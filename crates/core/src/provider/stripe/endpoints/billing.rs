use axum::{Json, body::Bytes, extract::State, http::StatusCode, response::IntoResponse};

use crate::provider::{
    ProviderState,
    stripe::{
        types::{ListParams, ListResponse, StripeBillingMeter, StripeMeterEvent},
        utils::generate_stripe_id,
    },
};

/// GET /v1/billing/meters - List billing meters
pub async fn list_meters(
    State(state): State<ProviderState>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(10).min(100) as usize;

    // Find starting position
    let start_idx = if let Some(starting_after) = params.starting_after {
        state
            .meters
            .iter()
            .position(|m| {
                let external_id = if state.use_sandbox {
                    m.sandboxes.get("default")
                } else {
                    m.deployed_id.as_ref()
                };
                external_id.map(|id| id == &starting_after).unwrap_or(false)
            })
            .map(|idx| idx + 1)
            .unwrap_or(0)
    } else {
        0
    };

    let end_idx = (start_idx + limit).min(state.meters.len());
    let meters_slice = &state.meters[start_idx..end_idx];

    let stripe_meters: Vec<StripeBillingMeter> = meters_slice
        .iter()
        .map(|m| StripeBillingMeter::from_meter(m, state.use_sandbox))
        .collect();

    let has_more = end_idx < state.meters.len();

    Json(ListResponse {
        object: "list".to_string(),
        data: stripe_meters,
        has_more,
        url: "/v1/billing/meters".to_string(),
    })
}

/// POST /v1/billing/meter_events - Record a meter event
pub async fn create_meter_event(
    State(_state): State<ProviderState>,
    body: Bytes,
) -> impl IntoResponse {
    // Parse form-encoded body manually
    let body_str = String::from_utf8_lossy(&body);

    // Extract event_name
    let event_name = body_str
        .split('&')
        .find(|part| part.starts_with("event_name="))
        .and_then(|part| part.strip_prefix("event_name="))
        .map(|name| urlencoding::decode(name).unwrap_or_default().to_string())
        .unwrap_or_else(|| "unknown_event".to_string());

    // Generate a mock meter event ID
    let event_id = generate_stripe_id("bmes");
    let created = chrono::Utc::now().timestamp();

    let meter_event = StripeMeterEvent {
        id: event_id,
        object: "billing.meter_event".to_string(),
        event_name,
        created,
        identifier: None,
    };

    (StatusCode::OK, Json(meter_event))
}
