//! SSE events endpoint for live event streaming
//!
//! Supports:
//! - `/events` - Live events only (polls DB)
//! - `/events?last=10` - Last 10 events, then live
//! - `/events?cursor=abc-123` - Events after cursor, then live
//! - `/events?stream_id=my-stream` - Stateful stream with server-side cursor
//!
//! This endpoint uses DB polling for serverless/Lambda compatibility,
//! eliminating the need for background consumer threads.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Extension,
    extract::Query,
    response::{
        IntoResponse,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use futures::stream::Stream;
use tracing::info;

use crate::{
    api::payment::{PaymentApiConfig, db::DbManager},
    events::EventStreamQuery,
};

/// Polling interval for checking new events in the database
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Maximum number of events to return per poll
const POLL_BATCH_SIZE: i64 = 100;

/// GET /events - SSE endpoint for live payment events
/// Uses DB polling for serverless compatibility
pub async fn handler(
    Extension(state): Extension<PaymentApiConfig>,
    Query(query): Query<EventStreamQuery>,
) -> impl IntoResponse {
    create_polling_sse_stream(
        Arc::clone(&state.db_manager),
        state.payment_stack_id.clone(),
        state.is_sandbox,
        query,
    )
}

/// Create an SSE stream that polls the database for new events
/// This approach works in serverless environments where background threads aren't available
fn create_polling_sse_stream(
    db_manager: Arc<DbManager>,
    payment_stack_id: String,
    is_sandbox: bool,
    query: EventStreamQuery,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream_id = query.stream_id.clone();

    info!(
        stream_id = ?stream_id,
        last = ?query.last,
        cursor = ?query.cursor,
        payment_stack_id = %payment_stack_id,
        is_sandbox = %is_sandbox,
        "Creating DB-polling SSE stream"
    );

    let stream = async_stream::stream! {
        // Determine initial cursor
        let mut current_cursor: Option<String> = query.cursor.clone();

        // Get initial replay events
        let replay_events = if let Some(ref cursor) = current_cursor {
            // Get events after cursor
            match db_manager.get_events_after_cursor(cursor, &payment_stack_id, is_sandbox, POLL_BATCH_SIZE) {
                Ok(events) => events,
                Err(e) => {
                    tracing::error!("Failed to get events after cursor: {}", e);
                    vec![]
                }
            }
        } else if let Some(last_n) = query.last {
            if last_n > 0 {
                // Get last N events
                match db_manager.get_last_events(&payment_stack_id, is_sandbox, last_n as i64) {
                    Ok(events) => events,
                    Err(e) => {
                        tracing::error!("Failed to get last events: {}", e);
                        vec![]
                    }
                }
            } else {
                // last=0 means no replay, start fresh
                vec![]
            }
        } else {
            // No replay requested, but we need to set cursor to latest event
            // so we only get NEW events in the poll loop
            match db_manager.get_last_events(&payment_stack_id, is_sandbox, 1) {
                Ok(events) => {
                    if let Some(latest) = events.first() {
                        current_cursor = Some(latest.event_id.clone());
                        info!(
                            cursor = %latest.event_id,
                            "Starting live-only stream from latest event"
                        );
                    }
                    vec![] // Don't replay anything
                }
                Err(_) => vec![]
            }
        };

        // Send replay events
        for event in replay_events {
            current_cursor = Some(event.event_id.clone());
            yield Ok(SseEvent::default()
                .id(event.event_id)
                .event("payment")
                .data(event.data_json));
        }

        info!(
            stream_id = ?stream_id,
            cursor = ?current_cursor,
            "Entering DB polling loop"
        );

        // Poll for new events
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            let new_events = if let Some(ref cursor) = current_cursor {
                match db_manager.get_events_after_cursor(cursor, &payment_stack_id, is_sandbox, POLL_BATCH_SIZE) {
                    Ok(events) => events,
                    Err(e) => {
                        tracing::error!("Failed to poll for new events: {}", e);
                        continue;
                    }
                }
            } else {
                // No cursor yet, get last few events
                match db_manager.get_last_events(&payment_stack_id, is_sandbox, POLL_BATCH_SIZE) {
                    Ok(events) => events,
                    Err(e) => {
                        tracing::error!("Failed to get events: {}", e);
                        continue;
                    }
                }
            };

            for event in new_events {
                info!(
                    stream_id = ?stream_id,
                    event_id = %event.event_id,
                    "Sending new event from DB poll"
                );
                current_cursor = Some(event.event_id.clone());
                yield Ok(SseEvent::default()
                    .id(event.event_id)
                    .event("payment")
                    .data(event.data_json));
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
