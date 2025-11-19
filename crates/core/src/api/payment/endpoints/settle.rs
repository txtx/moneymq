use std::sync::Arc;

use axum::{
    Extension,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, SettleRequest, SettleResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use tracing::{error, info};

use crate::api::payment::{FacilitatorState, endpoints::serialize_to_base64, networks};

/// POST /settle endpoint - settle a payment on-chain
pub async fn handler(
    Extension(state): Extension<Option<FacilitatorState>>,
    Json(request): Json<SettleRequest>,
) -> impl IntoResponse {
    let Some(state) = state else {
        return (
            StatusCode::NOT_FOUND,
            Json(SettleResponse {
                success: false,
                error_reason: Some(FacilitatorErrorReason::UnexpectedSettleError),
                payer: request.payment_requirements.pay_to.clone(),
                transaction: None,
                network: request.payment_requirements.network.clone(),
            }),
        );
    };

    info!(
        "Received settle request for network: {:?}",
        request.payment_requirements.network
    );

    // Verify network matches
    let Some(network_config) = state
        .config
        .networks
        .iter()
        .find_map(|(_, network_config)| {
            network_config
                .network()
                .eq(&request.payment_requirements.network)
                .then(|| network_config)
        })
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(SettleResponse {
                success: false,
                error_reason: Some(FacilitatorErrorReason::InvalidNetwork),
                payer: request.payment_requirements.pay_to.clone(),
                transaction: None,
                network: request.payment_requirements.network.clone(),
            }),
        );
    };

    // Verify payment payload network matches requirements
    if request.payment_payload.network != request.payment_requirements.network {
        return (
            StatusCode::BAD_REQUEST,
            Json(SettleResponse {
                success: false,
                error_reason: Some(FacilitatorErrorReason::InvalidNetwork),
                payer: request.payment_requirements.pay_to.clone(),
                transaction: None,
                network: request.payment_requirements.network.clone(),
            }),
        );
    }

    // Delegate to network-specific settlement
    let (status_code, response) = match network_config.network() {
        Network::Solana => {
            let rpc_client = Arc::new(RpcClient::new_with_commitment(
                network_config.rpc_url().to_string(),
                CommitmentConfig::confirmed(),
            ));
            match networks::solana::settle_solana_payment(
                &request,
                &network_config,
                &rpc_client,
                &state.kora_config,
                &state.signer_pool,
            )
            .await
            {
                Ok(response) => (StatusCode::OK, response),
                Err(e) => {
                    error!("Settlement failed: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        SettleResponse {
                            success: false,
                            error_reason: Some(FacilitatorErrorReason::FreeForm(e.to_string())),
                            payer: request.payment_requirements.pay_to.clone(),
                            transaction: None,
                            network: request.payment_requirements.network.clone(),
                        },
                    )
                }
            }
        }
    };

    let settle_request_base64 = serialize_to_base64(&request);
    let settle_response_base64 = serialize_to_base64(&response);

    let status = if response.success {
        "completed".into()
    } else {
        "failed".into()
    };
    let signature = response
        .transaction
        .as_ref()
        .map(|tx_hash| tx_hash.to_string());

    // Extract transaction for payment_hash lookup
    let transaction_str = match &request.payment_payload.payload {
        moneymq_types::x402::ExactPaymentPayload::Solana(payload) => &payload.transaction,
    };

    // Find transaction by payment_hash for idempotent settlement updates
    match state
        .db_manager
        .find_transaction_id_by_payment_hash(transaction_str)
    {
        Ok(Some(tx_id)) => {
            if let Err(e) = state.db_manager.update_transaction_after_settlement(
                tx_id,
                Some(status),
                signature,
                Some(settle_request_base64),
                Some(settle_response_base64),
            ) {
                error!("Failed to update transaction after settlement: {}", e);
            }
        }
        Ok(None) => {
            // Check if transaction is already settled (idempotent behavior)
            match state
                .db_manager
                .is_transaction_already_settled(transaction_str)
            {
                Ok(true) => {
                    tracing::debug!(
                        "Transaction already settled for payment_hash (idempotent settle request)"
                    );
                }
                Ok(false) => {
                    error!("No matching transaction found to update after settlement");
                }
                Err(e) => {
                    error!("Error checking if transaction is settled: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Error finding transaction for settlement: {}", e);
        }
    }

    (status_code, Json(response))
}
