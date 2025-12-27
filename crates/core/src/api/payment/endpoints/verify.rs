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

use crate::{
    api::payment::{PaymentApiConfig, endpoints::serialize_to_base64, networks},
    events::{
        PaymentFlow, PaymentVerificationFailedData, PaymentVerificationSucceededData,
        create_payment_verification_failed_event, create_payment_verification_succeeded_event,
    },
};

/// POST /verify endpoint - verify a payment payload
pub async fn handler(
    Extension(state): Extension<PaymentApiConfig>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    debug!("Verify endpoint called");

    debug!(
        "Verify endpoint: PaymentApiConfig loaded, payment_stack_id={}",
        state.payment_stack_id
    );

    debug!(
        "Received verify request for network: {:?}",
        request.payment_requirements.network
    );

    // Verify network matches
    let Some(network_config) =
        state
            .facilitator_config
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

    // Emit CloudEvent for verification result
    if let Some(ref sender) = state.event_sender {
        // Extract product_id from payment requirements extra metadata
        let product_id = request
            .payment_requirements
            .extra
            .as_ref()
            .and_then(|extra| extra.get("product"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let event = match &response {
            VerifyResponse::Valid { payer } => {
                let event_data = PaymentVerificationSucceededData {
                    payer: payer.to_string(),
                    amount: request.payment_requirements.max_amount_required.0.clone(),
                    network: format!("{:?}", request.payment_requirements.network),
                    product_id: product_id.clone(),
                    payment_flow: PaymentFlow::X402,
                };
                create_payment_verification_succeeded_event(event_data)
            }
            VerifyResponse::Invalid { reason, payer } => {
                let event_data = PaymentVerificationFailedData {
                    payer: payer.as_ref().map(|p| p.to_string()),
                    amount: request.payment_requirements.max_amount_required.0.clone(),
                    network: format!("{:?}", request.payment_requirements.network),
                    reason: format!("{:?}", reason),
                    product_id,
                    payment_flow: PaymentFlow::X402,
                };
                create_payment_verification_failed_event(event_data)
            }
        };
        if let Err(e) = sender.send(event) {
            error!("Failed to send verification event: {}", e);
        }
    }

    (status, Json(response))
}
