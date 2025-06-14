//! Odyssey chainspec parsing logic.
use alloy_primitives::U256;
use reth_op::{
    chainspec::{
        make_op_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainHardforks, ChainSpec,
        EthereumHardfork, ForkCondition, Hardfork, NamedChain, OpChainSpec,
    },
    primitives::SealedHeader,
};
// OpHardfork needs to be imported directly
use reth_cli::chainspec::{parse_genesis, ChainSpecParser};
use reth_optimism_forks::OpHardfork;
use std::sync::{Arc, LazyLock};

/// Odyssey forks.
pub static ODYSSEY_FORKS: LazyLock<ChainHardforks> = LazyLock::new(|| {
    ChainHardforks::new(vec![
        (EthereumHardfork::Frontier.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Homestead.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Dao.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Tangerine.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::SpuriousDragon.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Byzantium.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Constantinople.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Petersburg.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Istanbul.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(0)),
        (EthereumHardfork::London.boxed(), ForkCondition::Block(0)),
        (
            EthereumHardfork::Paris.boxed(),
            ForkCondition::TTD {
                fork_block: None,
                total_difficulty: U256::ZERO,
                activation_block_number: 0,
            },
        ),
        (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(0)),
        (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(0)),
        (OpHardfork::Regolith.boxed(), ForkCondition::Timestamp(0)),
        (OpHardfork::Bedrock.boxed(), ForkCondition::Block(0)),
        (OpHardfork::Ecotone.boxed(), ForkCondition::Timestamp(0)),
        (OpHardfork::Canyon.boxed(), ForkCondition::Timestamp(0)),
        (OpHardfork::Holocene.boxed(), ForkCondition::Timestamp(0)),
        (OpHardfork::Isthmus.boxed(), ForkCondition::Timestamp(0)),
        (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(0)),
    ])
});

/// Odyssey dev testnet specification.
pub static ODYSSEY_DEV: LazyLock<Arc<OpChainSpec>> = LazyLock::new(|| {
    OpChainSpec::new(ChainSpec {
        chain: Chain::from_named(NamedChain::Odyssey),
        genesis: serde_json::from_str(include_str!("../../../etc/dev-genesis.json"))
            .expect("Can't deserialize odyssey genesis json"),
        paris_block_and_final_difficulty: Some((0, U256::ZERO)),
        hardforks: ODYSSEY_FORKS.clone(),
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::ethereum()),
        deposit_contract: None,
        ..Default::default()
    })
    .into()
});

/// Odyssey main chain specification.
pub static ODYSSEY_MAINNET: LazyLock<Arc<OpChainSpec>> = LazyLock::new(|| {
    let genesis = serde_json::from_str(include_str!("../../../etc/odyssey-genesis.json"))
        .expect("Can't deserialize odyssey genesis json");
    let genesis_header = make_op_genesis_header(&genesis, &ODYSSEY_FORKS.clone());
    OpChainSpec::new(ChainSpec {
        chain: Chain::from_named(NamedChain::Odyssey),
        // genesis contains empty alloc field because state at first bedrock block is imported
        // manually from trusted source
        genesis,
        genesis_header: SealedHeader::seal_slow(genesis_header),
        paris_block_and_final_difficulty: Some((0, U256::ZERO)),
        hardforks: ODYSSEY_FORKS.clone(),
        base_fee_params: BaseFeeParamsKind::Variable(
            vec![
                (EthereumHardfork::London.boxed(), BaseFeeParams::optimism()),
                (OpHardfork::Canyon.boxed(), BaseFeeParams::optimism_canyon()),
            ]
            .into(),
        ),
        prune_delete_limit: 10000,
        ..Default::default()
    })
    .into()
});

/// Odyssey chain specification parser.
#[derive(Debug, Clone, Default)]
pub struct OdysseyChainSpecParser;

impl ChainSpecParser for OdysseyChainSpecParser {
    type ChainSpec = OpChainSpec;

    const SUPPORTED_CHAINS: &'static [&'static str] = &["odyssey", "dev"];

    fn parse(s: &str) -> eyre::Result<Arc<Self::ChainSpec>> {
        Ok(match s {
            "odyssey" => ODYSSEY_MAINNET.clone(),
            "dev" => ODYSSEY_DEV.clone(),
            s => {
                let chainspec = OpChainSpec::from(parse_genesis(s)?);
                Arc::new(chainspec)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::OdysseyChainSpecParser;
    use reth_cli::chainspec::ChainSpecParser;
    use reth_op::chainspec::EthereumHardforks;
    use reth_optimism_forks::OpHardforks;

    #[test]
    fn chainspec_parser_adds_prague() {
        let mut chainspec_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        chainspec_path.push("../../etc/odyssey-genesis.json");

        let chain_spec = OdysseyChainSpecParser::parse(&chainspec_path.to_string_lossy())
            .expect("could not parse chainspec");

        assert!(chain_spec.is_bedrock_active_at_block(0));
        assert!(
            chain_spec.is_prague_active_at_timestamp(0),
            "prague should be active at timestamp 0"
        );
    }
}
