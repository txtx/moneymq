use std::{str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use kora_lib::rpc_server::method::{
    sign_and_send_transaction::{SignAndSendTransactionRequest, sign_and_send_transaction},
    sign_transaction::{SignTransactionRequest, sign_transaction},
};
use moneymq_types::x402::{
    ExactPaymentPayload, MixedAddress, SettleRequest, SettleResponse, TransactionHash,
    VerifyRequest, VerifyResponse, config::facilitator::FacilitatorNetworkConfig,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_keypair::Pubkey;
use solana_transaction::{Transaction, versioned::VersionedTransaction};
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
    _config: &FacilitatorNetworkConfig,
    rpc_client: &Arc<RpcClient>,
) -> Result<VerifyResponse> {
    info!("Verifying Solana payment");
    let solana_payload = match &request.payment_payload.payload {
        ExactPaymentPayload::Solana(payload) => payload,
    };
    let request = SignTransactionRequest {
        transaction: solana_payload.transaction.clone(),
        signer_key: None,
        sig_verify: false,
    };
    let response = sign_transaction(rpc_client, request).await?;
    let signer_pubkey = Pubkey::from_str(&response.signer_pubkey)?;
    let payer = MixedAddress::Solana(signer_pubkey);
    info!("Payment verified successfully");
    Ok(VerifyResponse::Valid { payer })
}

/// Settle a Solana payment on-chain using Kora SDK
pub async fn settle_solana_payment(
    request: &SettleRequest,
    config: &FacilitatorNetworkConfig,
    rpc_client: &Arc<RpcClient>,
) -> Result<SettleResponse> {
    info!("Settling Solana payment");
    let solana_payload = match &request.payment_payload.payload {
        ExactPaymentPayload::Solana(payload) => payload,
    };

    let send_request = SignAndSendTransactionRequest {
        transaction: solana_payload.transaction.clone(),
        signer_key: None,
        sig_verify: false,
    };
    let response = sign_and_send_transaction(rpc_client, send_request).await?;

    // Decode the signed transaction once and extract both customer and signature
    use base64::Engine;
    let signed_tx_bytes = base64::engine::general_purpose::STANDARD
        .decode(&response.signed_transaction)
        .context("Failed to decode signed transaction from base64")?;

    // Deserialize the transaction
    let signed_tx: VersionedTransaction = bincode::deserialize(&signed_tx_bytes)
        .context("Failed to deserialize signed transaction")?;

    // Extract the transaction signature (first signature)
    let signature_bytes: [u8; 64] = signed_tx
        .signatures
        .first()
        .context("Transaction has no signatures")?
        .as_ref()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;

    let signature = bs58::encode(&signature_bytes).into_string();
    info!("Transaction settled with signature: {}", signature);

    let signer_pubkey = Pubkey::from_str(&response.signer_pubkey)?;
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
