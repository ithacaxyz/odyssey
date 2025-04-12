//! # Transaction Queue
//!
//! This module implements a queue system for processing transactions.
//! In the future, this will allow for transaction batching and delegations for improved efficiency.
//!
//! ## Architecture
//!
//! - `TransactionQueue` processes requests in a separate thread
//! - `QueuedTransactionRequest` represents a request in the queue with a channel for sending results
//!
//! ## Usage
//!
//! ```
//! let queue = TransactionQueue::new(upstream);
//! let tx_hash = queue.send_transaction(request).await?;
//! ```

use alloy_json_rpc::RpcObject;
use alloy_network::{Ethereum, TransactionBuilder, TransactionBuilder7702};
use alloy_primitives::TxHash;
use jsonrpsee::core::RpcResult;
use std::{fmt::Debug, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error};

use crate::{OdysseyWalletError, Upstream};

/// Represents a transaction request in the queue
pub struct QueuedTransactionRequest<T> {
    /// Transaction request
    pub request: T,
    /// Channel for sending the transaction execution result
    pub response_sender: mpsc::Sender<Result<TxHash, OdysseyWalletError>>,
}

/// Transaction queue that processes requests in a separate thread
#[derive(Debug)]
pub struct TransactionQueue<T> {
    /// Transaction request sender
    sender: mpsc::Sender<QueuedTransactionRequest<T>>,
}

impl<T> Clone for TransactionQueue<T> {
    fn clone(&self) -> Self {
        Self { sender: self.sender.clone() }
    }
}

impl<T> Default for TransactionQueue<T> {
    fn default() -> Self {
        let (tx, _) = mpsc::channel(1);
        Self { sender: tx }
    }
}

impl<T> TransactionQueue<T>
where
    T: Debug + Send + 'static,
{
    /// Creates a new transaction queue
    pub fn new<U>(upstream: Arc<Mutex<U>>) -> Self
    where
        U: Upstream<TxRequest = T> + Sync + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(100); // Buffer for 100 transactions

        // Start the queue processor in a separate thread
        tokio::spawn(Self::process_queue(rx, upstream));

        Self { sender: tx }
    }

    /// Sends a transaction to the queue and returns the result
    pub async fn send_transaction(&self, request: T) -> RpcResult<TxHash> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        // Send the request to the queue
        self.sender
            .send(QueuedTransactionRequest { request, response_sender: response_tx })
            .await
            .map_err(|_| {
                error!("Failed to enqueue transaction request");
                jsonrpsee::core::Error::internal_error()
            })?;

        // Wait for the execution result
        response_rx
            .recv()
            .await
            .ok_or_else(|| {
                error!("Transaction processor closed without sending response");
                jsonrpsee::core::Error::internal_error()
            })?
            .map_err(Into::into)
    }

    /// Processes transaction requests from the queue
    async fn process_queue<U>(
        mut rx: mpsc::Receiver<QueuedTransactionRequest<T>>,
        upstream: Arc<Mutex<U>>,
    ) where
        U: Upstream<TxRequest = T> + Sync + Send + 'static,
    {
        while let Some(tx_request) = rx.recv().await {
            debug!("Processing transaction from queue");

            let result = {
                let upstream = upstream.lock().await;
                // Here we can add logic for transaction batching
                upstream.sign_and_send(tx_request.request).await
            };

            // Send the result back to the client
            if let Err(e) = tx_request.response_sender.send(result).await {
                error!("Failed to send transaction result: {:?}", e);
            }
        }
    }
}
