use crate::OdysseyWalletError;
use alloy::{sol, sol_types::SolCall};
use alloy_network::{
    eip2718::Encodable2718, Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder,
};
use alloy_primitives::{bytes::BytesMut, Address, Bytes, ChainId, TxHash, TxKind};
use alloy_rpc_types::{BlockId, TransactionInput, TransactionRequest};
use alloy_rpc_types_beacon::BlsPublicKey;
use futures::{stream::FuturesUnordered, StreamExt};
use jsonrpsee::core::RpcResult;
use reth_rpc_eth_api::helpers::{EthCall, EthTransactions, FullEthApi, LoadFee, LoadState};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::Mutex,
    time::{interval, Interval},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::*;

const BATCH_DELEGATION_CONTRACT: Address = Address::new([0; 20]);

sol! {
    contract BlsAggregator {
        struct CallByUser {
            address user;
            bytes pubkey;
            bytes calls;
        }

        function executeAggregated(
            bytes calldata signature,
            CallByUser[] memory callsByUser
        ) external;
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BlsData {
    signature: Bytes,
    pubkey: BlsPublicKey,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct BlsTransactionBatcher<Eth> {
    chain_id: ChainId,
    /// Ethereum wallet. Idea is that a wallet used for BLS signature aggregation would be
    /// different than the one in the regular flow.
    wallet: EthereumWallet,
    interval: Interval,
    batch_gas_limit: u64,
    eth_api: Eth,
    transaction_stream: UnboundedReceiverStream<(TransactionRequest, BlsData)>,
    batch: VecDeque<(TransactionRequest, BlsData)>, // (sig, validated tx, request)
    /// Used to guard tx signing.
    permit: Arc<Mutex<()>>,
    /// The pending requests that were sent to the sequencer.
    pending_requests: FuturesUnordered<Pin<Box<dyn Future<Output = RpcResult<TxHash>> + Send>>>,
}

impl<Eth> BlsTransactionBatcher<Eth> {
    /// Create new BLS transaction batcher.
    pub fn new(
        chain_id: ChainId,
        wallet: EthereumWallet,
        period: Duration,
        batch_gas_limit: u64,
        eth_api: Eth,
        transaction_stream: UnboundedReceiverStream<(TransactionRequest, BlsData)>,
    ) -> Self {
        Self {
            chain_id,
            wallet,
            interval: interval(period),
            batch_gas_limit,
            eth_api,
            transaction_stream,
            batch: VecDeque::new(),
            permit: Arc::<Mutex<()>>::default(),
            pending_requests: FuturesUnordered::new(),
        }
    }
}

impl<Eth> Future for BlsTransactionBatcher<Eth>
where
    Eth: FullEthApi + Clone + Send + Sync + Unpin + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        loop {
            if let Poll::Ready(Some(item)) = this.transaction_stream.poll_next_unpin(cx) {
                this.batch.push_back(item);
                continue;
            }

            if let Poll::Ready(Some(pending)) = this.pending_requests.poll_next_unpin(cx) {
                match pending {
                    Ok(tx_hash) => {
                        debug!(target: "rpc::wallet::bls_aggregator", %tx_hash, "Batch transaction successfully submitted to the pool");
                    }
                    Err(err) => {
                        error!(target: "rpc::wallet::bls_aggregator", ?err, "Error sending batch transaction");
                    }
                }
                continue;
            }

            if this.interval.poll_tick(cx).is_ready() {
                let mut cumulative_gas = 0;
                let mut current_batch = Vec::new();
                while let Some((tx, _sig)) = this.batch.front() {
                    let tx_gas = tx.gas.unwrap();
                    if cumulative_gas + tx_gas > this.batch_gas_limit {
                        break;
                    }
                    cumulative_gas += tx_gas;
                    current_batch.push(this.batch.pop_front().unwrap());
                }

                if !current_batch.is_empty() {
                    let chain_id = this.chain_id;
                    let wallet = this.wallet.clone();
                    let eth_api = this.eth_api.clone();
                    let permit = this.permit.clone();
                    this.pending_requests.push(Box::pin(async move {
                        let permit = permit.lock().await;
                        construct_and_send_batch_transaction(
                            chain_id,
                            wallet,
                            eth_api,
                            current_batch,
                            cumulative_gas,
                        )
                        .await
                    }));
                }

                continue;
            }

            return Poll::Pending;
        }
    }
}

async fn construct_and_send_batch_transaction<Eth: FullEthApi>(
    chain_id: ChainId,
    wallet: EthereumWallet,
    eth_api: Eth,
    batch: Vec<(TransactionRequest, BlsData)>,
    cumulative_gas: u64,
) -> RpcResult<TxHash> {
    let signature = Bytes::new();
    let mut aggregated = HashMap::<Address, HashMap<BlsPublicKey, Vec<Bytes>>>::default();
    for (tx, bls_data) in batch {
        // TODO: aggregate signature
        let to_address = *tx.to.unwrap().to().unwrap();
        let input = tx.input.input().take().unwrap().clone();
        aggregated.entry(to_address).or_default().entry(bls_data.pubkey).or_default().push(input);
    }

    let mut calls_by_user = Vec::new();
    for (user, by_key) in aggregated {
        for (pubkey, inputs) in by_key {
            let mut calls = BytesMut::default();
            for input in inputs {
                calls.extend(input);
            }
            calls_by_user.push(BlsAggregator::CallByUser {
                user,
                pubkey: Bytes::copy_from_slice(pubkey.as_slice()),
                calls: calls.freeze().into(),
            });
        }
    }
    let input = Bytes::from(
        BlsAggregator::executeAggregatedCall { signature, callsByUser: calls_by_user }.abi_encode(),
    );

    let next_nonce = LoadState::next_available_nonce(
        &eth_api,
        NetworkWallet::<Ethereum>::default_signer_address(&wallet),
    )
    .await
    .map_err(Into::into)?;
    let mut request = TransactionRequest {
        chain_id: Some(chain_id),
        nonce: Some(next_nonce),
        to: Some(TxKind::Call(BATCH_DELEGATION_CONTRACT)),
        input: TransactionInput::from(input),
        gas: Some(cumulative_gas),
        ..Default::default()
    };

    let (estimate, base_fee) = tokio::join!(
        EthCall::estimate_gas_at(&eth_api, request.clone(), BlockId::latest(), None),
        LoadFee::eip1559_fees(&eth_api, None, None)
    );
    let estimate = estimate.map_err(Into::into)?;
    let (base_fee, _) = base_fee.map_err(Into::into)?;

    // Finish the request
    let max_priority_fee_per_gas = 1_000_000_000; // 1 gwei
    request.max_fee_per_gas = Some(base_fee.to::<u128>() + max_priority_fee_per_gas);
    request.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
    request.gas = Some(estimate.to());

    // build and sign
    let envelope = <TransactionRequest as TransactionBuilder<Ethereum>>::build::<EthereumWallet>(
        request, &wallet,
    )
    .await
    .map_err(|_| OdysseyWalletError::InvalidTransactionRequest)?;

    EthTransactions::send_raw_transaction(&eth_api, envelope.encoded_2718().into())
        .await
        .inspect_err(
            |err| warn!(target: "rpc::wallet::bls_aggregator", ?err, "Error adding batch tx to pool"),
        )
        .map_err(Into::into)
}
