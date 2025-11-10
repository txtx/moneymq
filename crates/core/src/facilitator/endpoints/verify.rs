use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, VerifyRequest, VerifyResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::{debug, error};

use crate::facilitator::{
    FacilitatorState,
    endpoints::{FacilitatorExtraContext, serialize_to_base64},
    networks,
};

/// POST /verify endpoint - verify a payment payload
pub async fn handler(
    State(state): State<FacilitatorState>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    debug!(
        "Received verify request for network: {:?}",
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
        debug!(
            "Invalid network in verify request: {:?}",
            request.payment_requirements.network
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::InvalidNetwork,
                payer: None,
            }),
        );
    };

    // Verify payment payload network matches requirements
    if request.payment_payload.network != request.payment_requirements.network {
        debug!(
            "Payment payload network does not match requirements: {:?} != {:?}",
            request.payment_payload.network, request.payment_requirements.network
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::InvalidNetwork,
                payer: None,
            }),
        );
    }

    // Delegate to network-specific verification
    let (status, response) = match network_config.network() {
        Network::Solana => {
            let rpc_client = Arc::new(RpcClient::new(network_config.rpc_url().to_string()));
            match networks::solana::verify_solana_payment(&request, &network_config, &rpc_client)
                .await
            {
                Ok(response) => (StatusCode::OK, response),
                Err(e) => {
                    error!("Verification failed: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        VerifyResponse::Invalid {
                            reason: FacilitatorErrorReason::FreeForm(e.to_string()),
                            payer: None,
                        },
                    )
                }
            }
        }
    };

    let verify_request_base64 = serialize_to_base64(&request);
    let verify_response_base64 = serialize_to_base64(&response);
    let payment_requirement_base64 = serialize_to_base64(&request.payment_requirements);
    let extra = match request.payment_requirements.extra.as_ref() {
        Some(extra) => {
            let extra: FacilitatorExtraContext = serde_json::from_value(extra.clone()).unwrap();
            Some(extra)
        }
        None => None,
    };

    if let Err(e) = state.db_manager.insert_transaction(
        extra,
        request.payment_requirements.max_amount_required.0.clone(),
        payment_requirement_base64,
        verify_request_base64,
        verify_response_base64,
    ) {
        error!("Failed to log transaction to database: {}", e);
    };

    (status, Json(response))
}
