use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, SettleRequest, SettleResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::{error, info};

use crate::facilitator::{FacilitatorState, networks};

/// POST /settle endpoint - settle a payment on-chain
pub async fn handler(
    State(state): State<FacilitatorState>,
    Json(request): Json<SettleRequest>,
) -> impl IntoResponse {
    info!(
        "Received settle request for network: {:?}",
        request.payment_requirements.network
    );

    // Verify network matches
    if request.payment_requirements.network != state.config.network {
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
    let rpc_client = Arc::new(RpcClient::new(state.config.rpc_url.clone()));
    match networks::solana::settle_solana_payment(&request, &state.config, &rpc_client).await {
        Ok(response) => (StatusCode::OK, Json(response)),
        Err(e) => {
            error!("Settlement failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SettleResponse {
                    success: false,
                    error_reason: Some(FacilitatorErrorReason::UnknownError),
                    payer: request.payment_requirements.pay_to.clone(),
                    transaction: None,
                    network: request.payment_requirements.network.clone(),
                }),
            )
        }
    }
}
