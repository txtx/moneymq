use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, VerifyRequest, VerifyResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::error;

use crate::facilitator::{FacilitatorState, networks};

/// POST /verify endpoint - verify a payment payload
pub async fn handler(
    State(state): State<FacilitatorState>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    println!(
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
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyResponse::Invalid {
                reason: FacilitatorErrorReason::InvalidNetwork,
                payer: None,
            }),
        );
    }

    // Delegate to network-specific verification
    match network_config.network() {
        Network::SolanaSurfnet | Network::SolanaMainnet => {
            let rpc_client = Arc::new(RpcClient::new(network_config.rpc_url().to_string()));
            match networks::solana::verify_solana_payment(&request, &network_config, &rpc_client)
                .await
            {
                Ok(response) => (StatusCode::OK, Json(response)),
                Err(e) => {
                    error!("Verification failed: {}", e);
                    (
                        StatusCode::BAD_REQUEST,
                        Json(VerifyResponse::Invalid {
                            reason: FacilitatorErrorReason::FreeForm(e.to_string()),
                            payer: None,
                        }),
                    )
                }
            }
        }
    }
}
