//! Odyssey rpc logic.
//!
//! `eth_` namespace overrides:
//!
//! - `eth_getProof` will _ONLY_ return the storage proofs _WITHOUT_ an account proof _IF_ targeting
//!   the withdrawal contract. Otherwise, it fallbacks to default behaviour.

use alloy_eips::BlockId;
use alloy_primitives::Address;
use alloy_rpc_types::serde_helpers::JsonStorageKey;
use alloy_rpc_types_eth::EIP1186AccountProofResponse;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use reth_errors::RethError;
use reth_rpc_eth_api::{
    helpers::{EthState, FullEthApi},
    FromEthApiError,
};
use reth_rpc_eth_types::EthApiError;
use reth_rpc_types_compat::proof::from_primitive_account_proof;
use reth_storage_api::BlockIdReader;
use reth_trie_common::AccountProof;
use tracing::trace;

const WITHDRAWAL_CONTRACT: alloy_primitives::Address =
    alloy_primitives::address!("4200000000000000000000000000000000000011");

/// Odyssey `eth_` RPC namespace overrides.
#[cfg_attr(not(test), rpc(server, namespace = "eth"))]
#[cfg_attr(test, rpc(server, client, namespace = "eth"))]
pub trait EthApiOverride {
    /// Returns the account and storage values of the specified account including the Merkle-proof.
    /// This call can be used to verify that the data you are pulling from is not tampered with.
    #[method(name = "getProof")]
    async fn get_proof(
        &self,
        address: Address,
        keys: Vec<JsonStorageKey>,
        block_number: Option<BlockId>,
    ) -> RpcResult<EIP1186AccountProofResponse>;
}

/// Implementation of the `eth_` namespace override
#[derive(Debug)]
pub struct EthApiExt<Eth> {
    eth_api: Eth,
}

impl<E> EthApiExt<E> {
    /// Create a new `EthApiExt` module.
    pub const fn new(eth_api: E) -> Self {
        Self { eth_api }
    }
}

#[async_trait]
impl<Eth> EthApiOverrideServer for EthApiExt<Eth>
where
    Eth: FullEthApi + Send + Sync + 'static,
{
    async fn get_proof(
        &self,
        address: Address,
        keys: Vec<JsonStorageKey>,
        block_number: Option<BlockId>,
    ) -> RpcResult<EIP1186AccountProofResponse> {
        trace!(target: "rpc::eth", ?address, ?keys, ?block_number, "Serving eth_getProof");

        // If we are targeting the withdrawal contract, then we only need to provide the storage
        // proofs for withdrawal.
        if address == WITHDRAWAL_CONTRACT {
            let _permit = self
                .eth_api
                .acquire_owned()
                .await
                .map_err(RethError::other)
                .map_err(EthApiError::Internal)?;

            return self
                .eth_api
                .spawn_blocking_io(move |this| {
                    let state = this.state_at_block_id(block_number.unwrap_or_default())?;
                    let storage_root = state
                        .storage_root(WITHDRAWAL_CONTRACT, Default::default())
                        .map_err(EthApiError::from_eth_err)?;
                    let storage_proofs = keys
                        .iter()
                        .map(|key| {
                            state.storage_proof(
                                WITHDRAWAL_CONTRACT,
                                key.as_b256(),
                                Default::default(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(EthApiError::from_eth_err)?;
                    let proof = AccountProof { storage_root, storage_proofs, ..Default::default() };
                    Ok(from_primitive_account_proof(proof, keys))
                })
                .await
                .map_err(Into::into);
        }

        EthState::get_proof(&self.eth_api, address, keys, block_number)
            .map_err(Into::into)?
            .await
            .map_err(Into::into)
    }
}
