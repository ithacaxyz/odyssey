//! Odyssey constants.

use alloy_primitives::{address, Address};

/// Withdrawal predeployed contract address.
///
/// [The L2ToL1MessagePasser](https://specs.optimism.io/protocol/withdrawals.html#the-l2tol1messagepasser-contract)
pub const WITHDRAWAL_CONTRACT: Address = address!("4200000000000000000000000000000000000016");
