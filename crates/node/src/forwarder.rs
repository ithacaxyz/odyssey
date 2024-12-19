//! P2P transaction forwarding

use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::Bytes;
use reth_network::{transactions::TransactionsHandle, NetworkPrimitives};
use reth_primitives_traits::transaction::signed::SignedTransaction;
use tokio::sync::broadcast::Receiver;
use tracing::trace;

/// Forwards raw transactions to the network.
pub async fn forward_raw_transactions<N: NetworkPrimitives>(
    txn: TransactionsHandle<N>,
    mut raw_txs: Receiver<Bytes>,
) {
    loop {
        if let Ok(raw_tx) = raw_txs.recv().await {
            if let Ok(tx) = N::BroadcastedTransaction::decode_2718(&mut raw_tx.as_ref()) {
                trace!(target: "rpc::rpc", tx=%tx.tx_hash(), "Forwarding raw transaction over p2p");
                txn.broadcast_transactions(Some(tx));
            }
        }
    }
}
