//! # Odyssey
//!
//! Odyssey is a testnet OP Stack rollup aimed at enabling experimentation of bleeding edge
//! Ethereum Research. It aims to showcase how Reth's pluggable and modularized architecture can
//! serve as a distribution channel for research ideas.
//!
//! ## Feature Flags
//!
//! - `jemalloc`: Uses [jemallocator](https://github.com/tikv/jemallocator) as the global allocator.
//!   This is **not recommended on Windows**. See [here](https://rust-lang.github.io/rfcs/1974-global-allocators.html#jemalloc)
//!   for more info.
//! - `jemalloc-prof`: Enables [jemallocator's](https://github.com/tikv/jemallocator) heap profiling
//!   and leak detection functionality. See [jemalloc's opt.prof](https://jemalloc.net/jemalloc.3.html#opt.prof)
//!   documentation for usage details. This is **not recommended on Windows**. See [here](https://rust-lang.github.io/rfcs/1974-global-allocators.html#jemalloc)
//!   for more info.
//! - `asm-keccak`: replaces the default, pure-Rust implementation of Keccak256 with one implemented
//!   in assembly; see [the `keccak-asm` crate](https://github.com/DaniPopes/keccak-asm) for more
//!   details and supported targets
//! - `min-error-logs`: Disables all logs below `error` level.
//! - `min-warn-logs`: Disables all logs below `warn` level.
//! - `min-info-logs`: Disables all logs below `info` level. This can speed up the node, since fewer
//!   calls to the logging component is made.
//! - `min-debug-logs`: Disables all logs below `debug` level.
//! - `min-trace-logs`: Disables all logs below `trace` level.

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use async_trait::async_trait;
use clap::Parser;
use eyre::Context;
use odyssey_node::{chainspec::OdysseyChainSpecParser, node::OdysseyNode};
use odyssey_wallet::{OdysseyWallet, OdysseyWalletApiServer};
use futures::StreamExt;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{error::INTERNAL_ERROR_CODE, ErrorObject, ErrorObjectOwned},
};
use reth_exex::{ExExContext, ExExNotification};
use reth_node_builder::{engine_tree_config::TreeConfig, EngineNodeLauncher, FullNodeComponents};
use reth_optimism_cli::Cli;
use reth_optimism_node::{args::RollupArgs, node::OptimismAddOns};
use reth_optimism_rpc::sequencer::SequencerClient;
use reth_provider::providers::BlockchainProvider2;
use tracing::{info, warn};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

#[doc(hidden)]
fn main() {
    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    if let Err(err) =
        Cli::<OdysseyChainSpecParser, RollupArgs>::parse().run(|builder, rollup_args| async move {
            let (rpc_tx, rpc_rx) = mpsc::unbounded_channel(); // For WallTimeExEx

            let node = builder
                .with_types_and_provider::<OdysseyNode, BlockchainProvider2<_>>()
                .with_components(OdysseyNode::components(&rollup_args))
                .with_add_ons(OptimismAddOns::new(rollup_args.sequencer_http.clone()))
                .extend_rpc_modules(move |ctx| {
                    // register sequencer tx forwarder
                    if let Some(sequencer_http) = rollup_args.sequencer_http {
                        ctx.registry
                            .eth_api()
                            .set_sequencer_client(SequencerClient::new(sequencer_http))?;
                    }

                    // register odyssey wallet namespace
                    if let Ok(sk) = std::env::var("EXP1_SK") {
                        let signer: PrivateKeySigner =
                            sk.parse().wrap_err("Invalid EXP0001 secret key.")?;
                        let wallet = EthereumWallet::from(signer);

                        let raw_delegations = std::env::var("EXP1_WHITELIST")
                            .wrap_err("No EXP0001 delegations specified")?;
                        let valid_delegations: Vec<Address> = raw_delegations
                            .split(',')
                            .map(|addr| Address::parse_checksummed(addr, None))
                            .collect::<Result<_, _>>()
                            .wrap_err("No valid EXP0001 delegations specified")?;

                        ctx.modules
                            .merge_configured(WallTimeRpcExt { to_exex: rpc_tx }.into_rpc())?;

                        ctx.modules.merge_configured(
                            OdysseyWallet::new(
                                ctx.provider().clone(),
                                wallet,
                                ctx.registry.eth_api().clone(),
                                ctx.config().chain.chain().id(),
                                valid_delegations,
                            )
                            .into_rpc(),
                        )?;

                        info!(target: "reth::cli", "EXP0001 wallet configured");
                    } else {
                        warn!(target: "reth::cli", "EXP0001 wallet not configured");
                    }

                    Ok(())
                })
                .install_exex("walltime", |ctx| async move {
                    Ok(WallTimeExEx::new(ctx, UnboundedReceiverStream::from(rpc_rx)))
                })
                .launch_with_fn(|builder| {
                    let engine_tree_config = TreeConfig::default()
                        .with_persistence_threshold(rollup_args.persistence_threshold)
                        .with_memory_block_buffer_target(rollup_args.memory_block_buffer_target);
                    let launcher = EngineNodeLauncher::new(
                        builder.task_executor().clone(),
                        builder.config().datadir(),
                        engine_tree_config,
                    );
                    builder.launch_with(launcher)
                })
                .await?;

            node.wait_for_node_exit().await
        })
    {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}

/// Returns the current unix epoch in milliseconds.
pub fn unix_epoch_ms() -> u64 {
    use std::time::SystemTime;
    let now = SystemTime::now();
    now.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|err| panic!("Current time {now:?} is invalid: {err:?}"))
        .as_millis() as u64
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
/// Data about the last block for WallTimeExEx.
pub struct BlockTimeData {
    /// Wall time of last block
    wall_time_ms: u64,
    /// Timestamp of last block (chain time)
    block_timestamp: u64,
}

/// The WallTimeExEx struct.
pub struct WallTimeExEx<Node: FullNodeComponents> {
    /// The context of the `ExEx`
    ctx: ExExContext<Node>,
    /// Incoming RPC requests.
    rpc_requests_stream: UnboundedReceiverStream<oneshot::Sender<WallTimeData>>,
    /// Time data of last block
    last_block_timedata: BlockTimeData,
}

impl<Node: FullNodeComponents> WallTimeExEx<Node> {
    fn new(
        ctx: ExExContext<Node>,
        rpc_requests_stream: UnboundedReceiverStream<oneshot::Sender<WallTimeData>>,
    ) -> Self {
        Self { ctx, rpc_requests_stream, last_block_timedata: BlockTimeData::default() }
    }
}

impl<Node: FullNodeComponents + Unpin> Future for WallTimeExEx<Node> {
    type Output = eyre::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        loop {
            if let Poll::Ready(Some(notification)) = this.ctx.notifications.poll_next_unpin(cx) {
                let notification = notification?;
                match &notification {
                    ExExNotification::ChainCommitted { new } => {
                        info!(committed_chain = ?new.range(), "Received commit");
                    }
                    ExExNotification::ChainReorged { old, new } => {
                        info!(from_chain = ?old.range(), to_chain = ?new.range(), "Received reorg");
                    }
                    ExExNotification::ChainReverted { old } => {
                        info!(reverted_chain = ?old.range(), "Received revert");
                    }
                };

                if let Some(committed_chain) = notification.committed_chain() {
                    this.last_block_timedata.block_timestamp = committed_chain.tip().timestamp;
                    this.last_block_timedata.wall_time_ms = unix_epoch_ms();
                }
                continue;
            }

            if let Poll::Ready(Some(tx)) = this.rpc_requests_stream.poll_next_unpin(cx) {
                let _ = tx.send(WallTimeData {
                    current_wall_time_ms: unix_epoch_ms(),
                    last_block_wall_time_ms: this.last_block_timedata.wall_time_ms,
                    last_block_timestamp: this.last_block_timedata.block_timestamp,
                });
                continue;
            }

            return Poll::Pending;
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
/// Data about the current time and the last block for WallTimeExEx.
pub struct WallTimeData {
    /// Wall time right now
    current_wall_time_ms: u64,
    /// Wall time of last block
    last_block_wall_time_ms: u64,
    /// Timestamp of last block (chain time)
    last_block_timestamp: u64,
}

#[cfg_attr(not(test), rpc(server, namespace = "ext"))]
#[cfg_attr(test, rpc(server, client, namespace = "ext"))]
trait WallTimeRpcExtApi {
    /// Return the wall time and block timestamp of the latest block.
    #[method(name = "getWallTimeData")]
    async fn get_timedata(&self) -> RpcResult<WallTimeData>;
}

#[derive(Debug)]
/// The WallTimeRpcExt struct.
pub struct WallTimeRpcExt {
    to_exex: mpsc::UnboundedSender<oneshot::Sender<WallTimeData>>,
}

#[async_trait]
impl WallTimeRpcExtApiServer for WallTimeRpcExt {
    async fn get_timedata(&self) -> RpcResult<WallTimeData> {
        let (tx, rx) = oneshot::channel();
        let _ = self.to_exex.send(tx).map_err(|_| rpc_internal_error())?;
        rx.await.map_err(|_| rpc_internal_error())
    }
}

#[inline]
fn rpc_internal_error() -> ErrorObjectOwned {
    ErrorObject::owned(INTERNAL_ERROR_CODE, "internal error", Some(""))
}

