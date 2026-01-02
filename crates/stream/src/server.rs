//! HTTP server implementing the durable streams protocol.

use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{delete, get, head, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::Deserialize;
use tokio::time::timeout;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, info, warn};

use crate::{
    cursor::{generate_response_cursor, parse_cursor, CursorOptions},
    store::{StoreError, StreamStore},
    types::{format_offset, ServerOptions, StreamConfig},
};

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<StreamStore>,
    pub options: ServerOptions,
}

/// Query parameters for GET requests.
#[derive(Debug, Deserialize)]
pub struct ReadQuery {
    pub offset: Option<String>,
    pub live: Option<String>,
    pub cursor: Option<String>,
}

/// Create the router with all stream endpoints.
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .expose_headers(Any);

    Router::new()
        // Stream operations on wildcard paths
        .route("/{*path}", put(handle_create))
        .route("/{*path}", head(handle_head))
        .route("/{*path}", get(handle_read))
        .route("/{*path}", post(handle_append))
        .route("/{*path}", delete(handle_delete))
        .layer(cors)
        .with_state(state)
}

/// PUT - Create a new stream
async fn handle_create(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let path = format!("/{}", path);
    debug!(path = %path, "Creating stream");

    // Parse headers
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let ttl_seconds = parse_ttl_header(&headers);
    let expires_at = parse_expires_at_header(&headers);

    // Validate TTL/expires-at conflict
    if ttl_seconds.is_some() && expires_at.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "Cannot specify both Stream-TTL and Stream-Expires-At",
        )
            .into_response();
    }

    // Read body for initial data
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Failed to read body").into_response();
        }
    };

    let initial_data = if body_bytes.is_empty() {
        None
    } else {
        Some(body_bytes)
    };

    let config = StreamConfig {
        content_type: content_type.clone(),
        ttl_seconds,
        expires_at,
        initial_data,
    };

    match state.store.create(&path, config) {
        Ok(created) => {
            let next_offset = state.store.get_current_offset(&path).unwrap_or_default();

            let mut response = Response::builder()
                .status(if created {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                })
                .header("Stream-Next-Offset", &next_offset)
                .header(header::LOCATION, &path);

            if let Some(ct) = content_type {
                response = response.header(header::CONTENT_TYPE, ct);
            }

            response.body(Body::empty()).unwrap().into_response()
        }
        Err(StoreError::ConfigMismatch) => (
            StatusCode::CONFLICT,
            "Stream already exists with different configuration",
        )
            .into_response(),
        Err(StoreError::TtlConflict) => (
            StatusCode::BAD_REQUEST,
            "Cannot specify both Stream-TTL and Stream-Expires-At",
        )
            .into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to create stream");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// HEAD - Get stream metadata
async fn handle_head(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<ReadQuery>,
) -> impl IntoResponse {
    let path = format!("/{}", path);

    let stream = match state.store.get(&path) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let start_offset = query.offset.as_deref().unwrap_or("-1");
    let etag = generate_etag(&path, start_offset, &stream.current_offset);

    let cursor_options = CursorOptions {
        interval_seconds: state.options.cursor_interval_seconds,
        epoch: state.options.cursor_epoch,
    };
    let client_cursor = query.cursor.as_ref().and_then(|c| parse_cursor(c));
    let response_cursor = generate_response_cursor(client_cursor, &cursor_options);

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header("Stream-Next-Offset", &stream.current_offset)
        .header("Stream-Cursor", response_cursor.to_string())
        .header(header::ETAG, etag);

    if let Some(ct) = &stream.content_type {
        response = response.header(header::CONTENT_TYPE, ct.as_str());
    }

    response.body(Body::empty()).unwrap().into_response()
}

/// GET - Read from stream (catch-up, long-poll, or SSE)
async fn handle_read(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<ReadQuery>,
) -> impl IntoResponse {
    let path = format!("/{}", path);

    // Check if stream exists
    let stream = match state.store.get(&path) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let start_offset = query.offset.as_deref().unwrap_or("-1");

    // Validate offset
    if start_offset.is_empty() {
        return (StatusCode::BAD_REQUEST, "Empty offset parameter").into_response();
    }

    let cursor_options = CursorOptions {
        interval_seconds: state.options.cursor_interval_seconds,
        epoch: state.options.cursor_epoch,
    };
    let client_cursor = query.cursor.as_ref().and_then(|c| parse_cursor(c));
    let response_cursor = generate_response_cursor(client_cursor, &cursor_options);

    // Handle different modes
    match query.live.as_deref() {
        Some("sse") => handle_sse(state, path, start_offset.to_string(), response_cursor).await,
        Some("long-poll") => {
            handle_long_poll(state, path, start_offset.to_string(), response_cursor).await
        }
        _ => {
            handle_catch_up(
                state,
                path,
                start_offset.to_string(),
                response_cursor,
                stream,
            )
            .await
        }
    }
}

/// Handle catch-up read (immediate response)
async fn handle_catch_up(
    state: AppState,
    path: String,
    offset: String,
    cursor: u64,
    stream: crate::types::Stream,
) -> Response {
    let result = match state.store.read(&path, &offset) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    let body = state.store.format_response(&path, &result.messages);
    let etag = generate_etag(&path, &offset, &result.next_offset);

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header("Stream-Next-Offset", &result.next_offset)
        .header("Stream-Cursor", cursor.to_string())
        .header("Stream-Up-To-Date", result.up_to_date.to_string())
        .header(header::ETAG, etag);

    if let Some(ct) = &stream.content_type {
        response = response.header(header::CONTENT_TYPE, ct.as_str());
    }

    response.body(Body::from(body)).unwrap()
}

/// Handle long-poll read (wait for new data)
async fn handle_long_poll(state: AppState, path: String, offset: String, cursor: u64) -> Response {
    // First, try to read any available data
    let result = match state.store.read(&path, &offset) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    // If we have data, return it immediately
    if !result.messages.is_empty() {
        let stream = state.store.get(&path);
        let body = state.store.format_response(&path, &result.messages);
        let etag = generate_etag(&path, &offset, &result.next_offset);

        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header("Stream-Next-Offset", &result.next_offset)
            .header("Stream-Cursor", cursor.to_string())
            .header("Stream-Up-To-Date", result.up_to_date.to_string())
            .header(header::ETAG, etag);

        if let Some(ct) = stream.and_then(|s| s.content_type) {
            response = response.header(header::CONTENT_TYPE, ct.as_str());
        }

        return response.body(Body::from(body)).unwrap();
    }

    // No data available, wait for new messages
    let mut rx = state.store.subscribe();
    let timeout_duration = Duration::from_millis(state.options.long_poll_timeout_ms);

    match timeout(timeout_duration, async {
        loop {
            match rx.recv().await {
                Ok(notification) if notification.path == path => {
                    return Some(notification);
                }
                Ok(_) => continue, // Different path
                Err(_) => return None,
            }
        }
    })
    .await
    {
        Ok(Some(_notification)) => {
            // New data arrived, read and return it
            let result = match state.store.read(&path, &offset) {
                Ok(r) => r,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };

            let stream = state.store.get(&path);
            let body = state.store.format_response(&path, &result.messages);
            let etag = generate_etag(&path, &offset, &result.next_offset);

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("Stream-Next-Offset", &result.next_offset)
                .header("Stream-Cursor", cursor.to_string())
                .header("Stream-Up-To-Date", result.up_to_date.to_string())
                .header(header::ETAG, etag);

            if let Some(ct) = stream.and_then(|s| s.content_type) {
                response = response.header(header::CONTENT_TYPE, ct.as_str());
            }

            response.body(Body::from(body)).unwrap()
        }
        Ok(None) | Err(_) => {
            // Timeout or channel closed
            let current_offset = state
                .store
                .get_current_offset(&path)
                .unwrap_or_else(|| format_offset(0, 0));

            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .header("Stream-Next-Offset", &current_offset)
                .header("Stream-Cursor", cursor.to_string())
                .header("Stream-Up-To-Date", "true")
                .body(Body::empty())
                .unwrap()
        }
    }
}

/// Handle SSE streaming
async fn handle_sse(state: AppState, path: String, offset: String, cursor: u64) -> Response {
    let cursor_options = CursorOptions {
        interval_seconds: state.options.cursor_interval_seconds,
        epoch: state.options.cursor_epoch,
    };

    let stream = create_sse_stream(state, path, offset, cursor, cursor_options);

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

fn create_sse_stream(
    state: AppState,
    path: String,
    mut offset: String,
    cursor: u64,
    cursor_options: CursorOptions,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let mut rx = state.store.subscribe();
        let mut current_cursor = cursor;

        // First, send any existing data as replay
        if let Ok(result) = state.store.read(&path, &offset) {
            for msg in &result.messages {
                // Send data event
                let data_str = String::from_utf8_lossy(&msg.data);
                yield Ok(Event::default().event("data").data(&data_str));

                // Update offset
                offset = result.next_offset.clone();
            }

            // Send control event with current state
            current_cursor = generate_response_cursor(Some(current_cursor), &cursor_options);
            let control = serde_json::json!({
                "streamNextOffset": result.next_offset,
                "streamCursor": current_cursor.to_string(),
                "upToDate": result.up_to_date
            });
            yield Ok(Event::default().event("control").data(control.to_string()));
        }

        // Then listen for new messages
        loop {
            let timeout_duration = Duration::from_millis(state.options.long_poll_timeout_ms);

            match timeout(timeout_duration, rx.recv()).await {
                Ok(Ok(notification)) if notification.path == path => {
                    // New data for our stream
                    if let Ok(result) = state.store.read(&path, &offset) {
                        for msg in &result.messages {
                            let data_str = String::from_utf8_lossy(&msg.data);
                            yield Ok(Event::default().event("data").data(&data_str));
                        }

                        offset = result.next_offset.clone();
                        current_cursor = generate_response_cursor(Some(current_cursor), &cursor_options);

                        let control = serde_json::json!({
                            "streamNextOffset": result.next_offset,
                            "streamCursor": current_cursor.to_string(),
                            "upToDate": result.up_to_date
                        });
                        yield Ok(Event::default().event("control").data(control.to_string()));
                    }
                }
                Ok(Ok(_)) => continue, // Different path
                Ok(Err(_)) => break, // Channel closed
                Err(_) => {
                    // Timeout - send keepalive with current state
                    current_cursor = generate_response_cursor(Some(current_cursor), &cursor_options);
                    let current_offset = state.store.get_current_offset(&path)
                        .unwrap_or_else(|| format_offset(0, 0));

                    let control = serde_json::json!({
                        "streamNextOffset": current_offset,
                        "streamCursor": current_cursor.to_string(),
                        "upToDate": true
                    });
                    yield Ok(Event::default().event("control").data(control.to_string()));
                }
            }
        }
    }
}

/// POST - Append data to stream
async fn handle_append(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let path = format!("/{}", path);

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());

    let seq = headers.get("Stream-Seq").and_then(|v| v.to_str().ok());

    // Read body
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Failed to read body").into_response();
        }
    };

    if body_bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "Empty body not allowed").into_response();
    }

    match state.store.append(&path, body_bytes, content_type, seq) {
        Ok(new_offset) => Response::builder()
            .status(StatusCode::OK)
            .header("Stream-Next-Offset", new_offset)
            .body(Body::empty())
            .unwrap()
            .into_response(),
        Err(StoreError::NotFound(p)) => {
            (StatusCode::NOT_FOUND, format!("Stream not found: {}", p)).into_response()
        }
        Err(StoreError::ContentTypeMismatch { expected, actual }) => (
            StatusCode::CONFLICT,
            format!(
                "Content-type mismatch: expected {}, got {}",
                expected, actual
            ),
        )
            .into_response(),
        Err(StoreError::SequenceConflict(msg)) => {
            (StatusCode::CONFLICT, format!("Sequence conflict: {}", msg)).into_response()
        }
        Err(StoreError::EmptyBody) => {
            (StatusCode::BAD_REQUEST, "Empty body not allowed").into_response()
        }
        Err(StoreError::EmptyArrayNotAllowed) => {
            (StatusCode::BAD_REQUEST, "Empty arrays are not allowed").into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to append to stream");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// DELETE - Delete a stream
async fn handle_delete(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let path = format!("/{}", path);

    if state.store.delete(&path) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// Generate an ETag for a read response.
fn generate_etag(path: &str, start_offset: &str, end_offset: &str) -> String {
    use base64::Engine;
    let path_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(path);
    format!("\"{}:{}:{}\"", path_b64, start_offset, end_offset)
}

/// Parse Stream-TTL header.
fn parse_ttl_header(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("Stream-TTL")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

/// Parse Stream-Expires-At header.
fn parse_expires_at_header(headers: &HeaderMap) -> Option<DateTime<Utc>> {
    headers
        .get("Stream-Expires-At")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Start the server.
pub async fn start_server(options: ServerOptions) -> std::io::Result<()> {
    let store = StreamStore::new();
    let state = AppState {
        store,
        options: options.clone(),
    };

    let router = create_router(state);

    let addr = format!("{}:{}", options.host, options.port);
    info!("Starting durable streams server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    fn create_test_app() -> Router {
        let state = AppState {
            store: StreamStore::new(),
            options: ServerOptions::default(),
        };
        create_router(state)
    }

    #[tokio::test]
    async fn test_create_stream() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/test/stream")
                    .header("Content-Type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        assert!(response.headers().contains_key("stream-next-offset"));
    }

    #[tokio::test]
    async fn test_create_idempotent() {
        let store = StreamStore::new();
        let state = AppState {
            store: store.clone(),
            options: ServerOptions::default(),
        };
        let app = create_router(state);

        // First create
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/test/stream")
                    .header("Content-Type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Second create (idempotent)
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/test/stream")
                    .header("Content-Type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let store = StreamStore::new();
        let state = AppState {
            store: store.clone(),
            options: ServerOptions::default(),
        };
        let app = create_router(state);

        // Create stream
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/test/stream")
                    .header("Content-Type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Append data
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test/stream")
                    .header("Content-Type", "text/plain")
                    .body(Body::from("hello world"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Read data
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/test/stream?offset=-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"hello world");
    }

    #[tokio::test]
    async fn test_delete_stream() {
        let store = StreamStore::new();
        let state = AppState {
            store: store.clone(),
            options: ServerOptions::default(),
        };
        let app = create_router(state);

        // Create stream
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/test/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Delete stream
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/test/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Try to read deleted stream
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/test/stream?offset=-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
