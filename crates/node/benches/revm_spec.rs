#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use alloy_primitives:: U256;
use reth_chainspec::{ ChainSpec, EthereumHardfork, ForkCondition, Head};
use reth_optimism_forks::OptimismHardfork;
use reth_revm::primitives::SpecId;
use std::{cmp::Ordering, time::Duration};


fn revm_spec_one(chain_spec: &ChainSpec, block: &Head) -> SpecId {
    if chain_spec.fork(EthereumHardfork::Prague).active_at_head(block) {
        reth_revm::primitives::PRAGUE_EOF
    } else if chain_spec.fork(OptimismHardfork::Granite).active_at_head(block) {
        reth_revm::primitives::GRANITE
    } else if chain_spec.fork(OptimismHardfork::Fjord).active_at_head(block) {
        reth_revm::primitives::FJORD
    } else if chain_spec.fork(OptimismHardfork::Ecotone).active_at_head(block) {
        reth_revm::primitives::ECOTONE
    } else if chain_spec.fork(OptimismHardfork::Canyon).active_at_head(block) {
        reth_revm::primitives::CANYON
    } else if chain_spec.fork(OptimismHardfork::Regolith).active_at_head(block) {
        reth_revm::primitives::REGOLITH
    } else if chain_spec.fork(OptimismHardfork::Bedrock).active_at_head(block) {
        reth_revm::primitives::BEDROCK
    } else if chain_spec.fork(EthereumHardfork::Prague).active_at_head(block) {
        reth_revm::primitives::PRAGUE
    } else if chain_spec.fork(EthereumHardfork::Cancun).active_at_head(block) {
        reth_revm::primitives::CANCUN
    } else if chain_spec.fork(EthereumHardfork::Shanghai).active_at_head(block) {
        reth_revm::primitives::SHANGHAI
    } else if chain_spec.fork(EthereumHardfork::Paris).active_at_head(block) {
        reth_revm::primitives::MERGE
    } else if chain_spec.fork(EthereumHardfork::London).active_at_head(block) {
        reth_revm::primitives::LONDON
    } else if chain_spec.fork(EthereumHardfork::Berlin).active_at_head(block) {
        reth_revm::primitives::BERLIN
    } else if chain_spec.fork(EthereumHardfork::Istanbul).active_at_head(block) {
        reth_revm::primitives::ISTANBUL
    } else if chain_spec.fork(EthereumHardfork::Petersburg).active_at_head(block) {
        reth_revm::primitives::PETERSBURG
    } else if chain_spec.fork(EthereumHardfork::Byzantium).active_at_head(block) {
        reth_revm::primitives::BYZANTIUM
    } else if chain_spec.fork(EthereumHardfork::SpuriousDragon).active_at_head(block) {
        reth_revm::primitives::SPURIOUS_DRAGON
    } else if chain_spec.fork(EthereumHardfork::Tangerine).active_at_head(block) {
        reth_revm::primitives::TANGERINE
    } else if chain_spec.fork(EthereumHardfork::Homestead).active_at_head(block) {
        reth_revm::primitives::HOMESTEAD
    } else if chain_spec.fork(EthereumHardfork::Frontier).active_at_head(block) {
        reth_revm::primitives::FRONTIER
    } else {
        panic!(
            "invalid hardfork chainspec: expected at least one hardfork, got {:?}",
            chain_spec.hardforks
        )
    }
}


fn revm_spec_two(chain_spec: &ChainSpec, block: &Head) -> SpecId {
    enum Hardfork {
        Ethereum(EthereumHardfork),
        Optimism(OptimismHardfork),
    }

    static HARDFORKS: [(Hardfork, SpecId); 20] =  [
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

fn chain_spec() -> ChainSpec {
    let mut chain_spec = ChainSpec::default();
  

chain_spec.hardforks.insert(EthereumHardfork::Frontier, ForkCondition::Block(0));
chain_spec.hardforks.insert(EthereumHardfork::Homestead, ForkCondition::Block(1_150_000));
chain_spec.hardforks.insert(EthereumHardfork::Tangerine, ForkCondition::Block(2_463_000));
chain_spec.hardforks.insert(EthereumHardfork::SpuriousDragon, ForkCondition::Block(2_675_000));
chain_spec.hardforks.insert(EthereumHardfork::Byzantium, ForkCondition::Block(4_370_000));
chain_spec.hardforks.insert(EthereumHardfork::Constantinople, ForkCondition::Block(7_280_000));
chain_spec.hardforks.insert(EthereumHardfork::Istanbul, ForkCondition::Block(9_069_000));
chain_spec.hardforks.insert(EthereumHardfork::Berlin, ForkCondition::Block(12_244_000));
chain_spec.hardforks.insert(EthereumHardfork::London, ForkCondition::Block(12_965_000));
chain_spec.hardforks.insert(EthereumHardfork::Paris, ForkCondition::TTD { 
    total_difficulty: U256::from(58_750_000_000_000_000_000_000u128),
    fork_block: Some(15_537_394),
});
chain_spec.hardforks.insert(EthereumHardfork::Shanghai, ForkCondition::Timestamp(1681338455));
chain_spec.hardforks.insert(EthereumHardfork::Cancun, ForkCondition::Timestamp(1710338135));

chain_spec
}

fn benchmark_revm_spec_opt(c: &mut Criterion) {
    let chain_spec = chain_spec();
    
    let mut group = c.benchmark_group("revm_spec_opt");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    for block_number in [1, 1_000_000, 10_000_000, 100_000_000] {
        let head = Head {
            number: block_number,
            timestamp: 0,
            difficulty: U256::ZERO,
            total_difficulty: U256::ZERO,
            hash: Default::default(),
        };
        group.bench_with_input(format!("optimized_block_{}", block_number), &head, |b, head| {
            b.iter(|| revm_spec_two(black_box(&chain_spec), black_box(head)))
        });
    }

    group.finish();
}

fn benchmark_revm_spec(c: &mut Criterion) {
    let chain_spec = chain_spec();
    
    let mut group = c.benchmark_group("revm_spec");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    for block_number in [1, 1_000_000, 10_000_000, 100_000_000] {
        let head = Head {
            number: block_number,
            timestamp: 0,
            difficulty: U256::ZERO,
            total_difficulty: U256::ZERO,
            hash: Default::default(),
        };
        group.bench_with_input(format!("original_block_{}", block_number), &head, |b, head| {
            b.iter(|| revm_spec_one(black_box(&chain_spec), black_box(head)))
        });
    }

    group.finish();
}


criterion_group!(benches, benchmark_revm_spec, benchmark_revm_spec_opt);
criterion_main!(benches);