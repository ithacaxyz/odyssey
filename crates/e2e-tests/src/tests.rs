use std::{collections::BTreeMap, sync::LazyLock};

use alloy::{
    eips::eip7702::Authorization,
    primitives::{address, b256, Address, B256, U256},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    signers::SignerSync,
};
use alloy_network::{TransactionBuilder, TransactionBuilder7702};
use alloy_rpc_types::{BlockNumberOrTag, EIP1186AccountProofResponse, TransactionRequest};
use alloy_signer_local::PrivateKeySigner;
use url::Url;

static REPLICA_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("REPLICA_RPC")
        .expect("failed to get REPLICA_RPC env var")
        .parse()
        .expect("failed to parse REPLICA_RPC env var")
});

static SEQUENCER_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("SEQUENCER_RPC")
        .expect("failed to get SEQUENCER_RPC env var")
        .parse()
        .expect("failed to parse SEQUENCER_RPC env var")
});

#[tokio::test]
async fn assert_chain_advances() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let block = ProviderBuilder::new().on_http(SEQUENCER_RPC.clone()).get_block_number().await?;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let new_block =
        ProviderBuilder::new().on_http(SEQUENCER_RPC.clone()).get_block_number().await?;

    assert!(new_block > block);

    Ok(())
}

#[tokio::test]
async fn test_wallet_api() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&b256!(
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
    ))?;

    let capabilities: BTreeMap<U256, BTreeMap<String, BTreeMap<String, Vec<Address>>>> =
        provider.client().request_noparams("wallet_getCapabilities").await?;

    let chain_id = U256::from(provider.get_chain_id().await?);

    let delegation_address =
        capabilities.get(&chain_id).unwrap().get("delegation").unwrap().get("addresses").unwrap()
            [0];

    let auth = Authorization {
        chain_id: provider.get_chain_id().await?,
        address: delegation_address,
        nonce: provider.get_transaction_count(signer.address()).await?,
    };

    let signature = signer.sign_hash_sync(&auth.signature_hash())?;
    let auth = auth.into_signed(signature);

    let tx =
        TransactionRequest::default().with_authorization_list(vec![auth]).with_to(signer.address());

    let tx_hash: B256 = provider.client().request("wallet_sendTransaction", vec![tx]).await?;

    let receipt = PendingTransactionBuilder::new(provider.clone(), tx_hash).get_receipt().await?;

    assert!(receipt.status());

    assert!(!provider.get_code_at(signer.address()).await?.is_empty());

    Ok(())
}

// This is new endpoint `odyssey_sendTransaction`, upper test will be deprecate in the future.
#[tokio::test]
async fn test_new_wallet_api() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&b256!(
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
    ))?;

    let capabilities: BTreeMap<U256, BTreeMap<String, BTreeMap<String, Vec<Address>>>> =
        provider.client().request_noparams("wallet_getCapabilities").await?;

    let chain_id = U256::from(provider.get_chain_id().await?);

    let delegation_address =
        capabilities.get(&chain_id).unwrap().get("delegation").unwrap().get("addresses").unwrap()
            [0];

    let auth = Authorization {
        chain_id: provider.get_chain_id().await?,
        address: delegation_address,
        nonce: provider.get_transaction_count(signer.address()).await?,
    };

    let signature = signer.sign_hash_sync(&auth.signature_hash())?;
    let auth = auth.into_signed(signature);

    let tx =
        TransactionRequest::default().with_authorization_list(vec![auth]).with_to(signer.address());

    let tx_hash: B256 = provider.client().request("odyssey_sendTransaction", vec![tx]).await?;

    let receipt = PendingTransactionBuilder::new(provider.clone(), tx_hash).get_receipt().await?;

    assert!(receipt.status());

    assert!(!provider.get_code_at(signer.address()).await?.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_withdrawal_proof_with_fallback() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    #[derive(Debug, Clone, serde::Serialize)]
    struct ProofParams {
        address: Address,
        keys: Vec<B256>,
        block: BlockNumberOrTag,
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&b256!(
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
    ))?;

    // Withdrawal contract will return an empty account proof, since it only handles storage proofs
    let withdrawal_contract_response: EIP1186AccountProofResponse = provider
        .client()
        .request(
            "eth_getProof",
            ProofParams {
                address: odyssey_constants::WITHDRAWAL_CONTRACT,
                keys: vec![B256::ZERO],
                block: BlockNumberOrTag::Latest,
            },
        )
        .await?;
    assert!(withdrawal_contract_response.account_proof.is_empty());
    assert!(!withdrawal_contract_response.storage_proof.is_empty());

    // If not targeting the withdrawal contract, it defaults back to the standard getProof
    // implementation
    let eoa_response: EIP1186AccountProofResponse = provider
        .client()
        .request(
            "eth_getProof",
            ProofParams {
                address: signer.address(),
                keys: vec![],
                block: BlockNumberOrTag::Latest,
            },
        )
        .await?;
    assert!(!eoa_response.account_proof.is_empty());

    Ok(())
}
