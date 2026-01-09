//! SSE events endpoint for live event streaming
//!
//! Supports:
//! - `/events` - Live events only
//! - `/events?last=10` - Last 10 events, then live
//! - `/events?cursor=abc-123` - Events after cursor, then live
//! - `/events?stream_id=my-stream` - Stateful stream with server-side cursor

use std::sync::Arc;

use axum::{Extension, extract::Query, response::IntoResponse};

use crate::api::payment::PaymentApiConfig;
use crate::events::{
    EventStreamContext, EventStreamQuery, StatefulEventBroadcaster, create_stateful_sse_stream,
};

/// GET /events - SSE endpoint for live payment events
pub async fn handler(
    Extension(state): Extension<PaymentApiConfig>,
    Query(query): Query<EventStreamQuery>,
) -> impl IntoResponse {
    let context = EventStreamContext {
        payment_stack_id: state.payment_stack_id.clone(),
        is_sandbox: state.is_sandbox,
    };

    let broadcaster = Arc::new(StatefulEventBroadcaster::new(
        100,
        Arc::clone(&state.db_manager),
        context,
    ));

    create_stateful_sse_stream(broadcaster, query)
}
