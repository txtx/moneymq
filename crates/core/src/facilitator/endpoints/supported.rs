use axum::{Json, extract::State, response::IntoResponse};
use moneymq_types::x402::{SupportedPaymentKind, SupportedResponse};

use crate::facilitator::FacilitatorState;

/// GET /supported endpoint - returns supported payment kinds
pub async fn handler(State(state): State<Option<FacilitatorState>>) -> impl IntoResponse {
    let Some(state) = state else {
        return Json(SupportedResponse { kinds: vec![] });
    };

    let kinds = state
        .config
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
