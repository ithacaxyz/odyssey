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

/// RPC endpoint URL for the replica node
static REPLICA_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("REPLICA_RPC")
        .expect("REPLICA_RPC environment variable is not set")
        .parse()
        .expect("REPLICA_RPC environment variable contains invalid URL")
});

/// RPC endpoint URL for the sequencer node
static SEQUENCER_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("SEQUENCER_RPC")
        .expect("SEQUENCER_RPC environment variable is not set")
        .parse()
        .expect("SEQUENCER_RPC environment variable contains invalid URL")
});

/// Test account private key
const TEST_PRIVATE_KEY: B256 =
    b256!("59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d");

/// Default delegation address for testing
const DEFAULT_DELEGATION_ADDRESS: &str = "0x90f79bf6eb2c4f870365e785982e1f101e93b906";

/// Tests if the chain is advancing by checking block numbers
#[tokio::test]
async fn assert_chain_advances() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(SEQUENCER_RPC.clone());

    let initial_block = provider.get_block_number().await?;

    // Wait for new block
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let new_block = provider.get_block_number().await?;

    assert!(
        new_block > initial_block,
        "Chain did not advance: initial block {initial_block}, current block {new_block}"
    );

    Ok(())
}

/// Tests the wallet API functionality with EIP-7702 delegation
#[tokio::test]
async fn test_wallet_api() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&TEST_PRIVATE_KEY)?;

    let delegation_address = Address::from_str(
        &std::env::var("DELEGATION_ADDRESS")
            .unwrap_or_else(|_| DEFAULT_DELEGATION_ADDRESS.to_string()),
    )?;

    // Create and sign authorization
    let auth = Authorization {
        chain_id: provider.get_chain_id().await?,
        address: delegation_address,
        nonce: provider.get_transaction_count(signer.address()).await?,
    };

    let signature = signer.sign_hash_sync(&auth.signature_hash())?;
    let auth = auth.into_signed(signature);

    // Prepare and send transaction
    let tx =
        TransactionRequest::default().with_authorization_list(vec![auth]).with_to(signer.address());

    let tx_hash: B256 = provider.client().request("wallet_sendTransaction", vec![tx]).await?;

    // Wait for and verify transaction receipt
    let receipt = PendingTransactionBuilder::new(provider.clone(), tx_hash).get_receipt().await?;

    assert!(receipt.status(), "Transaction failed");
    assert!(!provider.get_code_at(signer.address()).await?.unwrap_or_default().is_empty(), "No code at signer address");

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

/// Tests withdrawal proof functionality with fallback behavior
#[tokio::test]
async fn test_withdrawal_proof_with_fallback() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());

    // Get latest block for proof verification
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
