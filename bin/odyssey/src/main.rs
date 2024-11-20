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

use alloy_network::{Ethereum, EthereumWallet, NetworkWallet};
use alloy_signer_local::PrivateKeySigner;
use clap::Parser;
use eyre::Context;
use odyssey_node::{
    broadcaster::periodic_broadcaster,
    chainspec::OdysseyChainSpecParser,
    node::OdysseyNode,
    rpc::{EthApiExt, EthApiOverrideServer},
};
use odyssey_wallet::{OdysseyWallet, OdysseyWalletApiServer, RethNode};
use odyssey_walltime::{OdysseyWallTime, OdysseyWallTimeRpcApiServer};
use reth_node_builder::{engine_tree_config::TreeConfig, EngineNodeLauncher, NodeComponents};
use reth_optimism_cli::Cli;
use reth_optimism_node::{args::RollupArgs, node::OpAddOns};
use reth_provider::{providers::BlockchainProvider2, CanonStateSubscriptions};
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
            let wallet = sponsor()?;
            let address = wallet
                .as_ref()
                .map(<EthereumWallet as NetworkWallet<Ethereum>>::default_signer_address);

            let node = builder
                .with_types_and_provider::<OdysseyNode, BlockchainProvider2<_>>()
                .with_components(OdysseyNode::components(&rollup_args))
                .with_add_ons(OpAddOns::new(rollup_args.sequencer_http))
                .on_component_initialized(move |ctx| {
                    if let Some(address) = address {
                        ctx.task_executor.spawn(async move {
                            periodic_broadcaster(
                                address,
                                ctx.components.pool(),
                                ctx.components
                                    .network
                                    .transactions_handle()
                                    .await
                                    .expect("transactions_handle should be initialized"),
                            )
                            .await
                        });
                    }

                    Ok(())
                })
                .extend_rpc_modules(move |ctx| {
                    // override eth namespace
                    ctx.modules.replace_configured(
                        EthApiExt::new(ctx.registry.eth_api().clone()).into_rpc(),
                    )?;

                    // register odyssey wallet namespace
                    if let Some(wallet) = wallet {
                        ctx.modules.merge_configured(
                            OdysseyWallet::new(
                                RethNode::new(
                                    ctx.provider().clone(),
                                    ctx.registry.eth_api().clone(),
                                    wallet,
                                ),
                                ctx.config().chain.chain().id(),
                            )
                            .into_rpc(),
                        )?;
                    }

                    let walltime = OdysseyWallTime::spawn(ctx.provider().canonical_state_stream());
                    ctx.modules.merge_configured(walltime.into_rpc())?;
                    info!(target: "reth::cli", "Walltime configured");

                    Ok(())
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

/// Returns a [`EthereumWallet`] with the sponsor private key.
fn sponsor() -> eyre::Result<Option<EthereumWallet>> {
    std::env::var("EXP1_SK")
        .ok()
        .or_else(|| {
            warn!(target: "reth::cli", "EXP0001 wallet not configured");
            None
        })
        .map(|sk| {
            let wallet = sk
                .parse::<PrivateKeySigner>()
                .map(EthereumWallet::from)
                .wrap_err("Invalid EXP0001 secret key.")?;
            info!(target: "reth::cli", "EXP0001 wallet configured");
            Ok::<_, eyre::Report>(wallet)
        })
        .transpose()
}
