//! Sponsor periodic broadcaster

use alloy_primitives::Address;
use reth_network::{transactions::TransactionsHandle, NetworkPrimitives};
use reth_transaction_pool::TransactionPool;
use std::time::Duration;

/// Periodically broadcasts sponsored transactions from the transaction pool.
///
/// `p2p` broadcasting can potentially be flaky, and due to the p2p rules, some txs may never make
/// it to the sequencer, this can happen if a message is dropped internally when channel bounds are
/// enforced for example. So, we re-broadcast them every 10 minutes.
pub async fn periodic_broadcaster<P, N>(
    address: Address,
    pool: P,
    transactions_handle: TransactionsHandle<N>,
) where
    P: TransactionPool,
    N: NetworkPrimitives,
{
    let mut interval_timer = tokio::time::interval(Duration::from_secs(60));

    loop {
        let transactions =
            pool.get_transactions_by_sender(address).into_iter().map(|tx| *tx.hash()).collect();

        transactions_handle.propagate_transactions(transactions);

        interval_timer.tick().await;
    }
}
