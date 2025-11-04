use std::{str::FromStr, sync::Arc};

use anyhow::Result;
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
use tracing::info;

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
    let request = SignAndSendTransactionRequest {
        transaction: solana_payload.transaction.clone(),
        signer_key: None,
        sig_verify: false,
    };
    let response = sign_and_send_transaction(rpc_client, request).await?;
    let signer_pubkey = Pubkey::from_str(&response.signer_pubkey)?;
    let payer = MixedAddress::Solana(signer_pubkey);
    let signature = bs58::encode(&response.signed_transaction[..64]).into_string();
    let signature_bytes: [u8; 64] = {
        let bytes = bs58::decode(&signature).into_vec()?;
        bytes.try_into().unwrap()
    };
    info!("Transaction settled with signature: {}", signature);

    let tx_hash = TransactionHash::Solana(signature_bytes);

    Ok(SettleResponse {
        success: true,
        error_reason: None,
        payer,
        transaction: Some(tx_hash),
        network: config.network(),
    })
}
