use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, VerifyRequest, VerifyResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::{error, info};

use crate::facilitator::{FacilitatorState, networks};

/// POST /verify endpoint - verify a payment payload
pub async fn handler(
    State(state): State<FacilitatorState>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    info!(
        "Received verify request for network: {:?}",
        request.payment_requirements.network
    );

    // Verify network matches
    if request.payment_requirements.network != state.config.network {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::InvalidNetwork,
                payer: None,
            }),
        );
    }

    // Verify payment payload network matches requirements
    if request.payment_payload.network != request.payment_requirements.network {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::InvalidNetwork,
                payer: None,
            }),
        );
    }

    // Delegate to network-specific verification
    let rpc_client = Arc::new(RpcClient::new(state.config.rpc_url.clone()));
    match networks::solana::verify_solana_payment(&request, &state.config, &rpc_client).await {
        Ok(response) => (StatusCode::OK, Json(response)),
        Err(e) => {
            error!("Verification failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse::Invalid {
                    reason: FacilitatorErrorReason::UnknownError,
                    payer: None,
                }),
            )
        }
    }
}
