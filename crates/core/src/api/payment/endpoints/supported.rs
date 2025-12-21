use axum::{Extension, Json, response::IntoResponse};
use moneymq_types::x402::{SupportedPaymentKind, SupportedResponse};

use crate::api::payment::PaymentApiConfig;

/// GET /supported endpoint - returns supported payment kinds
pub async fn handler(Extension(state): Extension<PaymentApiConfig>) -> impl IntoResponse {
    let kinds = state
        .facilitator_config
        .networks
        .values()
        .map(|network_config| SupportedPaymentKind {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: network_config.network(),
            extra: network_config.extra(),
        })
        .collect::<Vec<SupportedPaymentKind>>();

    Json(SupportedResponse { kinds })
}
