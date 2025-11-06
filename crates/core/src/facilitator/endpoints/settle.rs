use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use moneymq_types::x402::{FacilitatorErrorReason, Network, SettleRequest, SettleResponse};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
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
    match network_config.network() {
        Network::Solana => {
            let rpc_client = Arc::new(RpcClient::new_with_commitment(
                network_config.rpc_url().to_string(),
                CommitmentConfig::confirmed(),
            ));
            match networks::solana::settle_solana_payment(&request, &network_config, &rpc_client)
                .await
            {
                Ok(response) => (StatusCode::OK, Json(response)),
                Err(e) => {
                    error!("Settlement failed: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SettleResponse {
                            success: false,
                            error_reason: Some(FacilitatorErrorReason::FreeForm(e.to_string())),
                            payer: request.payment_requirements.pay_to.clone(),
                            transaction: None,
                            network: request.payment_requirements.network.clone(),
                        }),
                    )
                }
            }
        }
    }
}
