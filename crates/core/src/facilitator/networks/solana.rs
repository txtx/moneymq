use anyhow::{Context, Result};
use kora_lib::{
    Config, SolanaSigner,
    signer::SignerPool,
    transaction::{TransactionUtil, VersionedTransactionOps, VersionedTransactionResolved},
};
use moneymq_types::x402::{
    ExactPaymentPayload, MixedAddress, SettleRequest, SettleResponse, TransactionHash,
    VerifyRequest, VerifyResponse, config::facilitator::FacilitatorNetworkConfig,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_keypair::Pubkey;
use solana_transaction::{Transaction, versioned::VersionedTransaction};
use std::sync::Arc;
use tracing::info;

/// Helper function to decode and extract payer from transaction
pub fn extract_customer_from_transaction(transaction_str: &str) -> Result<Pubkey> {
    info!("Decoding transaction of length: {}", transaction_str.len());

    // Try base64 first (common format), then fall back to base58
    use base64::Engine;
    let transaction_bytes =
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(transaction_str) {
            info!("Transaction decoded as base64");
            bytes
        } else if let Ok(bytes) = bs58::decode(transaction_str).into_vec() {
            info!("Transaction decoded as base58");
            bytes
        } else {
            return Err(anyhow::anyhow!(
                "Failed to decode transaction: not valid base64 or base58 (length: {})",
                transaction_str.len()
            ));
        };

    // Try to deserialize as VersionedTransaction first, then fall back to legacy Transaction
    let payer_pubkey = if let Ok(versioned_tx) =
        bincode::deserialize::<VersionedTransaction>(&transaction_bytes)
    {
        versioned_tx
            .message
            .static_account_keys()
            .get(1)
            .context("Transaction must have at least 2 accounts")?
            .clone()
    } else {
        let legacy_tx: Transaction = bincode::deserialize(&transaction_bytes)
            .context("Failed to deserialize transaction")?;
        legacy_tx
            .message
            .account_keys
            .get(1)
            .context("Transaction must have at least 2 accounts")?
            .clone()
    };

    Ok(payer_pubkey)
}

/// Verify a Solana payment payload
pub async fn verify_solana_payment(
    request: &VerifyRequest,
    rpc_client: &Arc<RpcClient>,
    kora_config: &Arc<Config>,
    signer_pool: &Arc<SignerPool>,
) -> Result<VerifyResponse> {
    info!("Verifying Solana payment");
    let solana_payload = match &request.payment_payload.payload {
        ExactPaymentPayload::Solana(payload) => payload,
    };
    let transaction = TransactionUtil::decode_b64_transaction(&solana_payload.transaction)?;

    // TODO: Check usage limit for transaction sender
    // UsageTracker::check_transaction_usage_limit(&config, &transaction).await?;

    let meta_signer = signer_pool.get_next_signer().unwrap();
    let mut resolved_transaction = VersionedTransactionResolved::from_transaction(
        &transaction,
        kora_config,
        rpc_client,
        false,
    )
    .await?;

    let _ = resolved_transaction
        .sign_transaction(&kora_config, &Arc::clone(&meta_signer.signer), rpc_client)
        .await?;

    let payer = MixedAddress::Solana(meta_signer.signer.pubkey());
    info!("Payment verified successfully");
    Ok(VerifyResponse::Valid { payer })
}

/// Settle a Solana payment on-chain using Kora SDK
pub async fn settle_solana_payment(
    request: &SettleRequest,
    config: &FacilitatorNetworkConfig,
    rpc_client: &Arc<RpcClient>,
    kora_config: &Arc<Config>,
    signer_pool: &Arc<SignerPool>,
) -> Result<SettleResponse> {
    info!("Settling Solana payment");
    let solana_payload = match &request.payment_payload.payload {
        ExactPaymentPayload::Solana(payload) => payload,
    };
    let transaction = TransactionUtil::decode_b64_transaction(&solana_payload.transaction)?;

    // TODO: Check usage limit for transaction sender
    // UsageTracker::check_transaction_usage_limit(&config, &transaction).await?;

    let meta_signer = signer_pool.get_next_signer().unwrap();
    let mut resolved_transaction = VersionedTransactionResolved::from_transaction(
        &transaction,
        kora_config,
        rpc_client,
        false,
    )
    .await?;

    let (signature, _encoded_transaction) = resolved_transaction
        .sign_and_send_transaction(kora_config, &Arc::clone(&meta_signer.signer), rpc_client)
        .await?;

    let signature_bytes: [u8; 64] = bs58::decode(&signature)
        .into_vec()
        .context("Failed to decode signature from base58")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;

    let signer_pubkey = meta_signer.signer.pubkey();
    let payer = MixedAddress::Solana(signer_pubkey);

    let tx_hash = TransactionHash::Solana(signature_bytes);

    Ok(SettleResponse {
        success: true,
        error_reason: None,
        payer,
        transaction: Some(tx_hash),
        network: config.network(),
    })
}
