//! # Odyssey Node types configuration
//!
//! The [`OdysseyNode`] type implements the [`NodeTypes`] trait, and configures the engine types
//! required for the optimism engine API.

use crate::evm::OdysseyEvmConfig;
use reth_network::{
    transactions::{TransactionPropagationMode, TransactionsManagerConfig},
    NetworkHandle, NetworkManager,
};
use reth_network_types::ReputationChangeWeights;
use reth_node_api::{FullNodeTypes, NodeTypesWithEngine};
use reth_node_builder::{
    components::{
        ComponentsBuilder, ExecutorBuilder, NetworkBuilder, PayloadServiceBuilder,
        PoolBuilderConfigOverrides,
    },
    BuilderContext, Node, NodeTypes,
};
use reth_optimism_chainspec::OpChainSpec;
use reth_optimism_node::{
    args::RollupArgs,
    node::{
        OptimismAddOns, OptimismConsensusBuilder, OptimismEngineValidatorBuilder,
        OptimismNetworkBuilder, OptimismPayloadBuilder, OptimismPoolBuilder,
    },
    OpExecutorProvider, OptimismEngineTypes,
};
use reth_payload_builder::PayloadBuilderHandle;
use reth_transaction_pool::{SubPoolLimit, TransactionPool, TXPOOL_MAX_ACCOUNT_SLOTS_PER_SENDER};
use std::time::Duration;

/// Type configuration for a regular Odyssey node.
#[derive(Debug, Clone, Default)]
pub struct OdysseyNode {
    /// Additional Optimism args
    pub args: RollupArgs,
}

impl OdysseyNode {
    /// Creates a new instance of the Optimism node type.
    pub const fn new(args: RollupArgs) -> Self {
        Self { args }
    }

    /// Returns the components for the given [`RollupArgs`].
    pub fn components<Node>(
        args: &RollupArgs,
    ) -> ComponentsBuilder<
        Node,
        OptimismPoolBuilder,
        OdysseyPayloadBuilder,
        OdysseyNetworkBuilder,
        OdysseyExecutorBuilder,
        OptimismConsensusBuilder,
        OptimismEngineValidatorBuilder,
    >
    where
        Node: FullNodeTypes<
            Types: NodeTypesWithEngine<Engine = OptimismEngineTypes, ChainSpec = OpChainSpec>,
        >,
    {
        ComponentsBuilder::default()
            .node_types::<Node>()
            .pool(OptimismPoolBuilder {
                pool_config_overrides: PoolBuilderConfigOverrides {
                    queued_limit: Some(SubPoolLimit::default() * 2),
                    pending_limit: Some(SubPoolLimit::default() * 2),
                    basefee_limit: Some(SubPoolLimit::default() * 2),
                    max_account_slots: Some(TXPOOL_MAX_ACCOUNT_SLOTS_PER_SENDER * 2),
                    ..Default::default()
                },
            })
            .payload(OdysseyPayloadBuilder::new(args.compute_pending_block))
            .network(OdysseyNetworkBuilder::new(OptimismNetworkBuilder {
                disable_txpool_gossip: args.disable_txpool_gossip,
                disable_discovery_v4: !args.discovery_v4,
            }))
            .executor(OdysseyExecutorBuilder::default())
            .consensus(OptimismConsensusBuilder::default())
            .engine_validator(OptimismEngineValidatorBuilder::default())
    }
}

/// Configure the node types
impl NodeTypes for OdysseyNode {
    type Primitives = ();
    type ChainSpec = OpChainSpec;
}

impl NodeTypesWithEngine for OdysseyNode {
    type Engine = OptimismEngineTypes;
}

impl<N> Node<N> for OdysseyNode
where
    N: FullNodeTypes<
        Types: NodeTypesWithEngine<Engine = OptimismEngineTypes, ChainSpec = OpChainSpec>,
    >,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        OptimismPoolBuilder,
        OdysseyPayloadBuilder,
        OdysseyNetworkBuilder,
        OdysseyExecutorBuilder,
        OptimismConsensusBuilder,
        OptimismEngineValidatorBuilder,
    >;

    type AddOns = OptimismAddOns;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        let Self { args } = self;
        Self::components(args)
    }

    fn add_ons(&self) -> Self::AddOns {
        OptimismAddOns::new(self.args.sequencer_http.clone())
    }
}

/// The Odyssey evm and executor builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct OdysseyExecutorBuilder;

impl<Node> ExecutorBuilder<Node> for OdysseyExecutorBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = OpChainSpec>>,
{
    type EVM = OdysseyEvmConfig;
    type Executor = OpExecutorProvider<Self::EVM>;

    async fn build_evm(
        self,
        ctx: &BuilderContext<Node>,
    ) -> eyre::Result<(Self::EVM, Self::Executor)> {
        let chain_spec = ctx.chain_spec();
        let evm_config = OdysseyEvmConfig::new(chain_spec.clone());
        let executor = OpExecutorProvider::new(chain_spec, evm_config.clone());

        Ok((evm_config, executor))
    }
}

/// The Odyssey payload service builder.
///
/// This service wraps the default Optimism payload builder, but replaces the default evm config
/// with Odyssey's own.
#[derive(Debug, Default, Clone)]
pub struct OdysseyPayloadBuilder {
    /// Inner Optimism payload builder service.
    inner: OptimismPayloadBuilder,
}

impl OdysseyPayloadBuilder {
    /// Create a new instance with the given `compute_pending_block` flag.
    pub const fn new(compute_pending_block: bool) -> Self {
        Self { inner: OptimismPayloadBuilder::new(compute_pending_block) }
    }
}

impl<Node, Pool> PayloadServiceBuilder<Node, Pool> for OdysseyPayloadBuilder
where
    Node: FullNodeTypes<
        Types: NodeTypesWithEngine<Engine = OptimismEngineTypes, ChainSpec = OpChainSpec>,
    >,
    Pool: TransactionPool + Unpin + 'static,
{
    async fn spawn_payload_service(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<PayloadBuilderHandle<OptimismEngineTypes>> {
        self.inner.spawn(OdysseyEvmConfig::new(ctx.chain_spec()), ctx, pool)
    }
}

/// The default odyssey network builder.
#[derive(Debug, Default, Clone)]
pub struct OdysseyNetworkBuilder {
    inner: OptimismNetworkBuilder,
}

impl OdysseyNetworkBuilder {
    /// Create a new instance based on the given op builder
    pub const fn new(network: OptimismNetworkBuilder) -> Self {
        Self { inner: network }
    }
}

impl<Node, Pool> NetworkBuilder<Node, Pool> for OdysseyNetworkBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = OpChainSpec>>,
    Pool: TransactionPool + Unpin + 'static,
{
    async fn build_network(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<NetworkHandle> {
        let mut network_config = self.inner.network_config(ctx)?;
        // this is rolled with limited trusted peers and we want ignore any reputation slashing
        network_config.peers_config.reputation_weights = ReputationChangeWeights::zero();
        network_config.peers_config.backoff_durations.low = Duration::from_secs(5);
        network_config.peers_config.backoff_durations.medium = Duration::from_secs(5);
        network_config.peers_config.max_backoff_count = u8::MAX;
        network_config.sessions_config.session_command_buffer = 500;
        network_config.sessions_config.session_event_buffer = 500;

        let txconfig = TransactionsManagerConfig {
            propagation_mode: TransactionPropagationMode::All,
            ..network_config.transactions_manager_config.clone()
        };
        let network = NetworkManager::builder(network_config).await?;
        let handle = ctx.start_network_with(network, pool, txconfig);

        Ok(handle)
    }
}
