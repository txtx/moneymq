use std::sync::Arc;

use axum::{
    Extension,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, VerifyRequest, VerifyResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use tracing::{debug, error};

use crate::api::payment::{FacilitatorState, endpoints::serialize_to_base64, networks};

/// POST /verify endpoint - verify a payment payload
pub async fn handler(
    Extension(state): Extension<Option<FacilitatorState>>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    debug!("Verify endpoint called");

    let Some(state) = state else {
        error!("Verify endpoint: FacilitatorState is None!");
        return (
            StatusCode::NOT_FOUND,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::FreeForm("Not found".into()),
                payer: None,
            }),
        );
    };

    debug!(
        "Verify endpoint: FacilitatorState loaded, payment_stack_id={}",
        state.payment_stack_id
    );

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
            let rpc_client = Arc::new(RpcClient::new_with_commitment(
                network_config.rpc_url().to_string(),
                CommitmentConfig::confirmed(),
            ));
            match networks::solana::verify_solana_payment(
                &request,
                &rpc_client,
                &state.kora_config,
                &state.signer_pool,
            )
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

    match state.db_manager.insert_transaction(
        &request,
        &response,
        payment_requirement_base64,
        verify_request_base64,
        verify_response_base64,
        &state.payment_stack_id,
        state.is_sandbox,
    ) {
        Ok(_) => {
            debug!("Transaction inserted successfully into database");
        }
        Err(e) => {
            error!("Failed to log transaction to database: {}", e);
        }
    };

    (status, Json(response))
}
