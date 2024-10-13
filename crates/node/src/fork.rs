use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use reth_chainspec::EthereumHardfork;
use reth_optimism_forks::OptimismHardfork;
use reth_revm::primitives::SpecId;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]


/// Represents a fork in either the Ethereum or Optimism network.
pub enum Fork {
    /// An Ethereum network fork.
    Ethereum(EthereumHardfork),
    /// An Optimism network fork.
    Optimism(OptimismHardfork),
}


/// A list of all forks in reverse chronological order (newest first).
pub static FORKS: Lazy<Vec<Fork>> = Lazy::new(|| {
    vec![
        // Optimism forks (from newest to old for efficient lookup)
        Fork::Optimism(OptimismHardfork::Granite),
        Fork::Optimism(OptimismHardfork::Fjord),
        Fork::Optimism(OptimismHardfork::Ecotone),
        Fork::Optimism(OptimismHardfork::Canyon),
        Fork::Optimism(OptimismHardfork::Regolith),
        Fork::Optimism(OptimismHardfork::Bedrock),
        
        // Ethereum forks (from newest to old for efficient lookup)
        Fork::Ethereum(EthereumHardfork::Prague),
        Fork::Ethereum(EthereumHardfork::Cancun),
        Fork::Ethereum(EthereumHardfork::Shanghai),
        Fork::Ethereum(EthereumHardfork::Paris),
        Fork::Ethereum(EthereumHardfork::London),
        Fork::Ethereum(EthereumHardfork::Berlin),
        Fork::Ethereum(EthereumHardfork::Istanbul),
        Fork::Ethereum(EthereumHardfork::Petersburg),
        Fork::Ethereum(EthereumHardfork::Byzantium),
        Fork::Ethereum(EthereumHardfork::SpuriousDragon),
        Fork::Ethereum(EthereumHardfork::Tangerine),
        Fork::Ethereum(EthereumHardfork::Homestead),
        Fork::Ethereum(EthereumHardfork::Frontier),
    ]
});


/// Maps each fork to its corresponding EVM specification ID.
pub static FORK_SPEC_MAP: Lazy<BTreeMap<Fork, SpecId>> = Lazy::new(|| {
    let mut map = BTreeMap::new();
    map.insert(Fork::Optimism(OptimismHardfork::Granite), SpecId::GRANITE);
    map.insert(Fork::Optimism(OptimismHardfork::Fjord), SpecId::FJORD);
    map.insert(Fork::Optimism(OptimismHardfork::Ecotone), SpecId::ECOTONE);
    map.insert(Fork::Optimism(OptimismHardfork::Canyon), SpecId::CANYON);
    map.insert(Fork::Optimism(OptimismHardfork::Regolith), SpecId::REGOLITH);
    map.insert(Fork::Optimism(OptimismHardfork::Bedrock), SpecId::BEDROCK);
    map.insert(Fork::Ethereum(EthereumHardfork::Prague), SpecId::PRAGUE_EOF);
    map.insert(Fork::Ethereum(EthereumHardfork::Cancun), SpecId::CANCUN);
    map.insert(Fork::Ethereum(EthereumHardfork::Shanghai), SpecId::SHANGHAI);
    map.insert(Fork::Ethereum(EthereumHardfork::Paris), SpecId::MERGE);
    map.insert(Fork::Ethereum(EthereumHardfork::London), SpecId::LONDON);
    map.insert(Fork::Ethereum(EthereumHardfork::Berlin), SpecId::BERLIN);
    map.insert(Fork::Ethereum(EthereumHardfork::Istanbul), SpecId::ISTANBUL);
    map.insert(Fork::Ethereum(EthereumHardfork::Petersburg), SpecId::PETERSBURG);
    map.insert(Fork::Ethereum(EthereumHardfork::Byzantium), SpecId::BYZANTIUM);
    map.insert(Fork::Ethereum(EthereumHardfork::SpuriousDragon), SpecId::SPURIOUS_DRAGON);
    map.insert(Fork::Ethereum(EthereumHardfork::Tangerine), SpecId::TANGERINE);
    map.insert(Fork::Ethereum(EthereumHardfork::Homestead), SpecId::HOMESTEAD);
    map.insert(Fork::Ethereum(EthereumHardfork::Frontier), SpecId::FRONTIER);
    map
});

