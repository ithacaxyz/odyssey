//! # Odyssey EVM configuration
//!
//! The [OdysseyEvmConfig] type implements the [ConfigureEvm] and [ConfigureEvmEnv] traits,
//! configuring the custom Odyssey precompiles and instructions.
//!
//! These trait implementations allow for custom precompiles and instructions to be implemented and
//! integrated in a reth node only with importing, without the need to fork the node or EVM
//! implementation.
//!
//! This currently configures the instructions defined in [EIP3074-instructions](https://github.com/paradigmxyz/eip3074-instructions), and the
//! precompiles defined by [`odyssey_precompile`].

use alloy_primitives::{Address, Bytes, TxKind, U256};
use odyssey_precompile::secp256r1;
use reth_chainspec::{ChainSpec, EthereumHardfork, Head};
use reth_node_api::{ConfigureEvm, ConfigureEvmEnv, NextBlockEnvAttributes};
use reth_optimism_chainspec::OpChainSpec;
use reth_optimism_forks::OptimismHardfork;
use reth_primitives::{
    revm_primitives::{CfgEnvWithHandlerCfg, TxEnv},
    transaction::FillTxEnv,
    Header, TransactionSigned,
};
use reth_revm::{
    handler::register::EvmHandler,
    inspector_handle_register,
    precompile::PrecompileSpecId,
    primitives::{
        AnalysisKind, BlobExcessGasAndPrice, BlockEnv, CfgEnv, Env, HandlerCfg, OptimismFields,
        SpecId,
    },
    ContextPrecompiles, Database, Evm, EvmBuilder, GetInspector,
};
use std::{cmp::Ordering, sync::Arc};

/// Custom EVM configuration
#[derive(Debug, Clone)]
pub struct OdysseyEvmConfig {
    chain_spec: Arc<OpChainSpec>,
}

impl OdysseyEvmConfig {
    /// Creates a new Odyssey EVM configuration with the given chain spec.
    pub const fn new(chain_spec: Arc<OpChainSpec>) -> Self {
        Self { chain_spec }
    }

    /// Sets the precompiles to the EVM handler
    ///
    /// This will be invoked when the EVM is created via [ConfigureEvm::evm] or
    /// [ConfigureEvm::evm_with_inspector]
    ///
    /// This will use the default mainnet precompiles and add additional precompiles.
    fn set_precompiles<EXT, DB>(handler: &mut EvmHandler<'_, EXT, DB>)
    where
        DB: Database,
    {
        // first we need the evm spec id, which determines the precompiles
        let spec_id = handler.cfg.spec_id;

        // install the precompiles
        handler.pre_execution.load_precompiles = Arc::new(move || {
            let mut loaded_precompiles: ContextPrecompiles<DB> =
                ContextPrecompiles::new(PrecompileSpecId::from_spec_id(spec_id));

            loaded_precompiles.extend(secp256r1::precompiles());

            loaded_precompiles
        });
    }
}

impl ConfigureEvmEnv for OdysseyEvmConfig {
    type Header = Header;

    fn fill_tx_env(&self, tx_env: &mut TxEnv, transaction: &TransactionSigned, sender: Address) {
        transaction.fill_tx_env(tx_env, sender);
    }

    fn fill_tx_env_system_contract_call(
        &self,
        env: &mut Env,
        caller: Address,
        contract: Address,
        data: Bytes,
    ) {
        env.tx = TxEnv {
            caller,
            transact_to: TxKind::Call(contract),
            // Explicitly set nonce to None so revm does not do any nonce checks
            nonce: None,
            gas_limit: 30_000_000,
            value: U256::ZERO,
            data,
            // Setting the gas price to zero enforces that no value is transferred as part of the
            // call, and that the call will not count against the block's gas limit
            gas_price: U256::ZERO,
            // The chain ID check is not relevant here and is disabled if set to None
            chain_id: None,
            // Setting the gas priority fee to None ensures the effective gas price is derived from
            // the `gas_price` field, which we need to be zero
            gas_priority_fee: None,
            access_list: Vec::new(),
            // blob fields can be None for this tx
            blob_hashes: Vec::new(),
            max_fee_per_blob_gas: None,
            authorization_list: None,
            optimism: OptimismFields {
                source_hash: None,
                mint: None,
                is_system_transaction: Some(false),
                // The L1 fee is not charged for the EIP-4788 transaction, submit zero bytes for the
                // enveloped tx size.
                enveloped_tx: Some(Bytes::default()),
            },
        };

        // ensure the block gas limit is >= the tx
        env.block.gas_limit = U256::from(env.tx.gas_limit);

        // disable the base fee check for this call by setting the base fee to zero
        env.block.basefee = U256::ZERO;
    }

    fn fill_cfg_env(
        &self,
        cfg_env: &mut CfgEnvWithHandlerCfg,
        header: &Header,
        total_difficulty: U256,
    ) {
        let spec_id = revm_spec(
            &self.chain_spec,
            &Head {
                number: header.number,
                timestamp: header.timestamp,
                difficulty: header.difficulty,
                total_difficulty,
                hash: Default::default(),
            },
        );

        cfg_env.chain_id = self.chain_spec.chain().id();
        cfg_env.perf_analyse_created_bytecodes = AnalysisKind::Analyse;

        cfg_env.handler_cfg.spec_id = spec_id;
        cfg_env.handler_cfg.is_optimism = true;
    }

    fn fill_block_env(&self, block_env: &mut BlockEnv, header: &Self::Header, after_merge: bool) {
        block_env.number = U256::from(header.number);
        block_env.coinbase = header.beneficiary;
        block_env.timestamp = U256::from(header.timestamp);
        if after_merge {
            block_env.prevrandao = Some(header.mix_hash);
            block_env.difficulty = U256::ZERO;
        } else {
            block_env.difficulty = header.difficulty;
            block_env.prevrandao = None;
        }
        block_env.basefee = U256::from(header.base_fee_per_gas.unwrap_or_default());
        block_env.gas_limit = U256::from(header.gas_limit);

        // EIP-4844 excess blob gas of this block, introduced in Cancun
        if let Some(excess_blob_gas) = header.excess_blob_gas {
            block_env.set_blob_excess_gas_and_price(excess_blob_gas);
        }
    }

    fn next_cfg_and_block_env(
        &self,
        parent: &Self::Header,
        attributes: NextBlockEnvAttributes,
    ) -> (CfgEnvWithHandlerCfg, BlockEnv) {
        // configure evm env based on parent block
        let cfg = CfgEnv::default().with_chain_id(self.chain_spec.chain().id());

        // ensure we're not missing any timestamp based hardforks
        let spec_id = revm_spec(
            &self.chain_spec,
            &Head {
                number: parent.number + 1,
                timestamp: attributes.timestamp,
                ..Default::default()
            },
        );

        // if the parent block did not have excess blob gas (i.e. it was pre-cancun), but it is
        // cancun now, we need to set the excess blob gas to the default value
        let blob_excess_gas_and_price = parent
            .next_block_excess_blob_gas()
            .or_else(|| {
                if spec_id.is_enabled_in(SpecId::CANCUN) {
                    // default excess blob gas is zero
                    Some(0)
                } else {
                    None
                }
            })
            .map(BlobExcessGasAndPrice::new);

        let block_env = BlockEnv {
            number: U256::from(parent.number + 1),
            coinbase: attributes.suggested_fee_recipient,
            timestamp: U256::from(attributes.timestamp),
            difficulty: U256::ZERO,
            prevrandao: Some(attributes.prev_randao),
            gas_limit: U256::from(parent.gas_limit),
            // calculate basefee based on parent block's gas usage
            basefee: U256::from(
                parent
                    .next_block_base_fee(
                        self.chain_spec.base_fee_params_at_timestamp(attributes.timestamp),
                    )
                    .unwrap_or_default(),
            ),
            // calculate excess gas based on parent block's blob gas usage
            blob_excess_gas_and_price,
        };

        let cfg_with_handler_cfg;
        {
            cfg_with_handler_cfg = CfgEnvWithHandlerCfg {
                cfg_env: cfg,
                handler_cfg: HandlerCfg { spec_id, is_optimism: true },
            };
        }

        (cfg_with_handler_cfg, block_env)
    }
}

impl ConfigureEvm for OdysseyEvmConfig {
    type DefaultExternalContext<'a> = ();

    fn evm<DB: Database>(&self, db: DB) -> Evm<'_, Self::DefaultExternalContext<'_>, DB> {
        EvmBuilder::default()
            .with_db(db)
            .optimism()
            // add additional precompiles
            .append_handler_register(Self::set_precompiles)
            .build()
    }

    fn evm_with_inspector<DB, I>(&self, db: DB, inspector: I) -> Evm<'_, I, DB>
    where
        DB: Database,
        I: GetInspector<DB>,
    {
        EvmBuilder::default()
            .with_db(db)
            .with_external_context(inspector)
            .optimism()
            // add additional precompiles
            .append_handler_register(Self::set_precompiles)
            .append_handler_register(inspector_handle_register)
            .build()
    }

    fn default_external_context<'a>(&self) -> Self::DefaultExternalContext<'a> {}
}

/// Determine the revm spec ID from the current block and reth chainspec.
fn revm_spec(chain_spec: &ChainSpec, block: &Head) -> SpecId {
    enum Hardfork {
        Ethereum(EthereumHardfork),
        Optimism(OptimismHardfork),
    }

    const HARDFORKS: &[(Hardfork, SpecId)] = &[
        (Hardfork::Ethereum(EthereumHardfork::Prague), SpecId::PRAGUE_EOF),
        (Hardfork::Optimism(OptimismHardfork::Granite), SpecId::GRANITE),
        (Hardfork::Optimism(OptimismHardfork::Fjord), SpecId::FJORD),
        (Hardfork::Optimism(OptimismHardfork::Ecotone), SpecId::ECOTONE),
        (Hardfork::Optimism(OptimismHardfork::Canyon), SpecId::CANYON),
        (Hardfork::Optimism(OptimismHardfork::Regolith), SpecId::REGOLITH),
        (Hardfork::Optimism(OptimismHardfork::Bedrock), SpecId::BEDROCK),
        (Hardfork::Ethereum(EthereumHardfork::Prague), SpecId::PRAGUE),
        (Hardfork::Ethereum(EthereumHardfork::Cancun), SpecId::CANCUN),
        (Hardfork::Ethereum(EthereumHardfork::Shanghai), SpecId::SHANGHAI),
        (Hardfork::Ethereum(EthereumHardfork::Paris), SpecId::MERGE),
        (Hardfork::Ethereum(EthereumHardfork::London), SpecId::LONDON),
        (Hardfork::Ethereum(EthereumHardfork::Berlin), SpecId::BERLIN),
        (Hardfork::Ethereum(EthereumHardfork::Istanbul), SpecId::ISTANBUL),
        (Hardfork::Ethereum(EthereumHardfork::Petersburg), SpecId::PETERSBURG),
        (Hardfork::Ethereum(EthereumHardfork::Byzantium), SpecId::BYZANTIUM),
        (Hardfork::Ethereum(EthereumHardfork::SpuriousDragon), SpecId::SPURIOUS_DRAGON),
        (Hardfork::Ethereum(EthereumHardfork::Tangerine), SpecId::TANGERINE),
        (Hardfork::Ethereum(EthereumHardfork::Homestead), SpecId::HOMESTEAD),
        (Hardfork::Ethereum(EthereumHardfork::Frontier), SpecId::FRONTIER),
    ];

    let mut left = 0;
    let mut right = HARDFORKS.len() - 1;

    while left <= right {
        let mid = left + (right - left) / 2;
        let (ref fork, spec_id) = HARDFORKS[mid];

        let is_active = match fork {
            Hardfork::Ethereum(f) => chain_spec.fork(*f).active_at_head(block),
            Hardfork::Optimism(f) => chain_spec.fork(*f).active_at_head(block),
        };

        match is_active.cmp(&true) {
            Ordering::Equal => return spec_id,
            Ordering::Greater => right = mid - 1,
            Ordering::Less => left = mid + 1,
        }
    }

    panic!(
        "invalid hardfork chainspec: expected at least one hardfork, got {:?}",
        chain_spec.hardforks
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reth_chainspec::{Chain, ChainSpecBuilder, EthereumHardfork};
    use reth_primitives::{
        revm_primitives::{BlockEnv, CfgEnv, SpecId},
        ForkCondition,
    };
    use std::time::Instant;

    #[test]
    fn test_fill_cfg_and_block_env() {
        let start = Instant::now();
        let mut cfg_env = CfgEnvWithHandlerCfg::new_with_spec_id(CfgEnv::default(), SpecId::LATEST);
        let mut block_env = BlockEnv::default();
        let header = Header::default();
        let chain_spec = Arc::new(OpChainSpec::new(
            ChainSpecBuilder::default()
                .chain(Chain::optimism_mainnet())
                .genesis(Default::default())
                .with_fork(EthereumHardfork::Frontier, ForkCondition::Block(0))
                .build(),
        ));
        let total_difficulty = U256::ZERO;

        OdysseyEvmConfig::new(chain_spec.clone()).fill_cfg_and_block_env(
            &mut cfg_env,
            &mut block_env,
            &header,
            total_difficulty,
        );

        assert_eq!(cfg_env.chain_id, chain_spec.chain().id());
        let stop = start.elapsed();
        println!("{:?}", stop);
    }
}
