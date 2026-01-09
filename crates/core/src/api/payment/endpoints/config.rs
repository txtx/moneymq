use axum::{Extension, Json, response::IntoResponse};
use serde::Serialize;

use crate::api::payment::PaymentApiConfig;

/// Solana network configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaConfig {
    /// RPC endpoint URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,
}

/// x402 protocol configuration
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X402Config {
    /// Solana network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solana: Option<SolanaConfig>,
}

/// Response for the /config endpoint
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// Whether the facilitator is running in sandbox mode
    pub is_sandbox: bool,
    /// x402 protocol configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402: Option<X402Config>,
}

/// GET /config endpoint - returns facilitator configuration
pub async fn handler(Extension(state): Extension<PaymentApiConfig>) -> impl IntoResponse {
    // Get the RPC URL from the first network config
    let rpc_url = state
        .facilitator_config
        .networks
        .values()
        .next()
        .map(|network| network.rpc_url().to_string());

    Json(ConfigResponse {
        is_sandbox: state.is_sandbox,
        x402: Some(X402Config {
            solana: Some(SolanaConfig { rpc_url }),
        }),
    })
}
