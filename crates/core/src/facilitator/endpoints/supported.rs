use axum::{extract::State, response::IntoResponse, Json};
use moneymq_types::x402::{SupportedPaymentKind, SupportedResponse};

use crate::facilitator::FacilitatorState;

/// GET /supported endpoint - returns supported payment kinds
pub async fn handler(State(state): State<FacilitatorState>) -> impl IntoResponse {
    let kinds = vec![SupportedPaymentKind {
        x402_version: 1,
        scheme: "exact".to_string(),
        network: state.config.network.clone(),
        extra: None,
    }];

    Json(SupportedResponse { kinds })
}
