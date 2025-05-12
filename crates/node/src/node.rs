//! # Odyssey Node types configuration
//!
//! The [`OdysseyNode`] type implements the [`NodeTypes`] trait, and configures the engine types
//! required for the optimism engine API.

use op_alloy_consensus::OpPooledTransaction;
use reth_network::{
    transactions::{
        config::TransactionPropagationKind, TransactionPropagationMode, TransactionsManagerConfig,
    },
    NetworkHandle, NetworkManager, PeersInfo,
};
use reth_network_types::ReputationChangeWeights;
use reth_node_api::{FullNodeTypes, TxTy};
use reth_node_builder::{
    components::{
        BasicPayloadServiceBuilder, ComponentsBuilder, NetworkBuilder, PoolBuilderConfigOverrides,
    },
    BuilderContext, Node, NodeAdapter, NodeComponentsBuilder, NodeTypes,
};
use reth_optimism_chainspec::OpChainSpec;
use reth_optimism_node::{
    args::RollupArgs,
    node::{
        OpAddOns, OpConsensusBuilder, OpExecutorBuilder, OpNetworkBuilder, OpPayloadBuilder,
        OpPoolBuilder, OpStorage,
    },
    OpEngineTypes, OpNetworkPrimitives,
};
use reth_optimism_payload_builder::config::OpDAConfig;
use reth_optimism_primitives::OpPrimitives;
use reth_transaction_pool::{
    PoolTransaction, SubPoolLimit, TransactionPool, TXPOOL_MAX_ACCOUNT_SLOTS_PER_SENDER,
};
use reth_trie_db::MerklePatriciaTrie;
use std::time::Duration;
use tracing::info;

/// Type configuration for a regular Odyssey node.
#[derive(Debug, Clone, Default)]
pub struct OdysseyNode {
    /// Additional Optimism args
    pub args: RollupArgs,
    /// Data availability configuration for the OP builder.
    ///
    /// Used to throttle the size of the data availability payloads (configured by the batcher via
    /// the `miner_` api).
    ///
    /// By default, no throttling is applied.
    pub da_config: OpDAConfig,
}

impl OdysseyNode {
    /// Creates a new instance of the Optimism node type.
    pub fn new(args: RollupArgs) -> Self {
        Self { args, da_config: OpDAConfig::default() }
    }

    /// Configure the data availability configuration for the OP builder.
    pub fn with_da_config(mut self, da_config: OpDAConfig) -> Self {
        self.da_config = da_config;
        self
    }

    /// Returns the components for the given [`RollupArgs`].
    pub fn components<Node>(
        &self,
    ) -> ComponentsBuilder<
        Node,
        OpPoolBuilder,
        BasicPayloadServiceBuilder<OpPayloadBuilder>,
        OdysseyNetworkBuilder,
        OpExecutorBuilder,
        OpConsensusBuilder,
    >
    where
        Node: FullNodeTypes<
            Types: NodeTypes<
                Payload = OpEngineTypes,
                ChainSpec = OpChainSpec,
                Primitives = OpPrimitives,
            >,
        >,
    {
        let RollupArgs { disable_txpool_gossip, compute_pending_block, discovery_v4, .. } =
            self.args;

        let mut pool_builder = OpPoolBuilder::default();
        pool_builder.enable_tx_conditional = true;
        pool_builder.pool_config_overrides = PoolBuilderConfigOverrides {
            queued_limit: Some(SubPoolLimit::default() * 2),
            pending_limit: Some(SubPoolLimit::default() * 2),
            basefee_limit: Some(SubPoolLimit::default() * 2),
            max_account_slots: Some(TXPOOL_MAX_ACCOUNT_SLOTS_PER_SENDER * 2),
            ..Default::default()
        };
        let payload_builder = BasicPayloadServiceBuilder::new(
            OpPayloadBuilder::new(compute_pending_block).with_da_config(self.da_config.clone()),
        );

        ComponentsBuilder::default()
            .node_types::<Node>()
            .pool(pool_builder)
            .executor(OpExecutorBuilder::default())
            .payload(payload_builder)
            .consensus(OpConsensusBuilder::default())
            .network(OdysseyNetworkBuilder::new(OpNetworkBuilder {
                disable_txpool_gossip,
                disable_discovery_v4: !discovery_v4,
            }))
    }
}

/// Configure the node types
impl NodeTypes for OdysseyNode {
    type Primitives = OpPrimitives;
    type ChainSpec = OpChainSpec;
    type StateCommitment = MerklePatriciaTrie;
    type Storage = OpStorage;
    type Payload = OpEngineTypes;
}

impl<N> Node<N> for OdysseyNode
where
    N: FullNodeTypes<
        Types: NodeTypes<
            Payload = OpEngineTypes,
            ChainSpec = OpChainSpec,
            Primitives = OpPrimitives,
            Storage = OpStorage,
        >,
    >,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        OpPoolBuilder,
        BasicPayloadServiceBuilder<OpPayloadBuilder>,
        OdysseyNetworkBuilder,
        OpExecutorBuilder,
        OpConsensusBuilder,
    >;

    type AddOns =
        OpAddOns<NodeAdapter<N, <Self::ComponentsBuilder as NodeComponentsBuilder<N>>::Components>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        Self::components(self)
    }

    fn add_ons(&self) -> Self::AddOns {
        Self::AddOns::builder()
            .with_sequencer(self.args.sequencer.clone())
            .with_da_config(self.da_config.clone())
            .with_enable_tx_conditional(self.args.enable_tx_conditional)
            .build()
    }
}

/// The default odyssey network builder.
#[derive(Debug, Default, Clone)]
pub struct OdysseyNetworkBuilder {
    inner: OpNetworkBuilder,
}

impl OdysseyNetworkBuilder {
    /// Create a new instance based on the given op builder
    pub const fn new(network: OpNetworkBuilder) -> Self {
        Self { inner: network }
    }
}

impl<Node, Pool> NetworkBuilder<Node, Pool> for OdysseyNetworkBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = OpChainSpec, Primitives = OpPrimitives>>,
    Pool: TransactionPool<
            Transaction: PoolTransaction<
                Consensus = TxTy<Node::Types>,
                Pooled = OpPooledTransaction,
            >,
        > + Unpin
        + 'static,
{
    type Network = NetworkHandle<OpNetworkPrimitives>;

    async fn build_network(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<Self::Network> {
        let mut network_config = self.inner.network_config(ctx)?;
        // this is rolled with limited trusted peers, and we want to ignore any reputation slashing
        network_config.peers_config.reputation_weights = ReputationChangeWeights::zero();
        network_config.peers_config.backoff_durations.low = Duration::from_secs(5);
        network_config.peers_config.backoff_durations.medium = Duration::from_secs(5);
        network_config.peers_config.backoff_durations.high = Duration::from_secs(5);
        network_config.peers_config.max_backoff_count = u8::MAX;
        network_config.sessions_config.session_command_buffer = 500;
        network_config.sessions_config.session_event_buffer = 500;

        let tx_config = TransactionsManagerConfig {
            propagation_mode: TransactionPropagationMode::All,
            ..network_config.transactions_manager_config.clone()
        };
        let network = NetworkManager::<OpNetworkPrimitives>::builder(network_config).await?;
        let handle: NetworkHandle<OpNetworkPrimitives> =
            ctx.start_network_with(network, pool, tx_config, TransactionPropagationKind::default());
        info!(target: "reth::cli", enode=%handle.local_node_record(), "P2P networking initialized");
        Ok(handle)
    }
}
