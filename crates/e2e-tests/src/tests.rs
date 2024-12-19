use std::{str::FromStr, sync::LazyLock};

use alloy::{
    eips::eip7702::Authorization,
    primitives::{b256, Address, B256},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    signers::SignerSync,
};
use alloy_network::{TransactionBuilder, TransactionBuilder7702};
use alloy_rpc_types::{Block, BlockNumberOrTag, EIP1186AccountProofResponse, TransactionRequest};
use alloy_signer_local::PrivateKeySigner;
use reth_primitives_traits::Account;
use reth_trie_common::{AccountProof, StorageProof};
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

    let delegation_address = Address::from_str(
        &std::env::var("DELEGATION_ADDRESS")
            .unwrap_or_else(|_| "0x90f79bf6eb2c4f870365e785982e1f101e93b906".to_string()),
    )
    .unwrap();

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

    let delegation_address = Address::from_str(
        &std::env::var("DELEGATION_ADDRESS")
            .unwrap_or_else(|_| "0x90f79bf6eb2c4f870365e785982e1f101e93b906".to_string()),
    )
    .unwrap();

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

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let block: Block = provider
        .client()
        .request("eth_getBlockByNumber", (BlockNumberOrTag::Latest, false))
        .await?;
    let block_number = BlockNumberOrTag::Number(block.header.number);

    // Withdrawal contract will return an empty account proof, since it only handles storage proofs
    let withdrawal_contract_response: EIP1186AccountProofResponse = provider
        .client()
        .request(
            "eth_getProof",
            (odyssey_common::WITHDRAWAL_CONTRACT, vec![B256::ZERO], block_number),
        )
        .await?;

    assert!(withdrawal_contract_response.account_proof.is_empty());
    assert!(!withdrawal_contract_response.storage_proof.is_empty());

    let storage_root = withdrawal_contract_response.storage_hash;
    for proof in withdrawal_contract_response.storage_proof {
        StorageProof::new(proof.key.as_b256()).with_proof(proof.proof).verify(storage_root)?
    }

    // If not targeting the withdrawal contract, it defaults back to the standard getProof
    // implementation
    let signer = PrivateKeySigner::from_bytes(&b256!(
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
    ))?;

    let eoa_response: EIP1186AccountProofResponse = provider
        .client()
        .request("eth_getProof", (signer.address(), [0; 0], block_number))
        .await
        .unwrap();

    assert!(!eoa_response.account_proof.is_empty());
    AccountProof {
        address: signer.address(),
        info: Some(Account {
            nonce: eoa_response.nonce,
            balance: eoa_response.balance,
            bytecode_hash: Some(eoa_response.code_hash),
        }),
        proof: eoa_response.account_proof,
        ..Default::default()
    }
    .verify(block.header.state_root)?;

    Ok(())
}
