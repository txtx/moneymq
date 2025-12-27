pub mod catalog;
pub mod payment;
pub mod sandbox;

use std::sync::Arc;

use axum::{Extension, Router, extract::Query, routing::get};
use catalog::CatalogState;
use payment::PaymentApiConfig;
// Re-export commonly used types
pub use sandbox::{NetworksConfig, NetworksConfigError};
use tower_http::cors::{Any, CorsLayer};

use crate::events::{EventStreamQuery, StatefulEventBroadcaster, create_stateful_sse_stream};

/// SSE endpoint handler for purchase events with cursor support
///
/// Query parameters:
/// - `cursor`: Resume from this event ID (exclusive)
/// - `last`: Replay the last N events before live streaming
/// - `stream_id`: Optional stream ID for stateful streams (server-side cursor persistence)
///
/// Example:
/// - `/events` - Live events only (stateless)
/// - `/events?last=10` - Last 10 events, then live (stateless)
/// - `/events?cursor=abc-123` - Events after abc-123, then live (stateless)
/// - `/events?stream_id=my-stream` - Stateful stream with server-side cursor
/// - `/events?stream_id=my-stream&last=10` - Stateful stream, replay last 10 on first connect
async fn sse_handler(
    Extension(broadcaster): Extension<Arc<StatefulEventBroadcaster>>,
    Query(query): Query<EventStreamQuery>,
) -> impl axum::response::IntoResponse {
    create_stateful_sse_stream(broadcaster, query)
}

/// Create a combined router that includes both catalog and payment APIs
///
/// # Arguments
/// * `catalog_state` - State for catalog endpoints
/// * `payment_api_config` - Optional state for payment/facilitator endpoints
/// * `event_broadcaster` - Optional stateful event broadcaster for SSE endpoint
/// * `extra_routes` - Optional additional routes to merge (e.g., IAC routes from CLI)
pub fn create_combined_router(
    catalog_state: CatalogState,
    payment_api_config: PaymentApiConfig,
    event_broadcaster: Option<Arc<StatefulEventBroadcaster>>,
    extra_routes: Option<Router<()>>,
) -> Router<()> {
    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create the catalog router (uses Extension layer internally)
    let catalog_router = catalog::create_router(catalog_state.clone());

    // Create root-level routes that need both CatalogState and PaymentApiConfig
    let root_routes: Router<()> = Router::new()
        .route("/config", get(sandbox::config::get_config))
        .route("/sandbox/accounts", get(sandbox::list_accounts))
        .layer(Extension(catalog_state))
        .layer(Extension(payment_api_config.clone()));

    // Start with root-level routes (health, fallback) and merge stateful routes
    let mut app = catalog::create_root_router()
        .merge(root_routes)
        .nest("/catalog/v1", catalog_router);

    // Mount the payment/facilitator API under /payment/v1 if provided
    let payment_router = payment::create_router(payment_api_config);
    app = app.nest("/payment/v1", payment_router);

    // Add SSE endpoint for purchase events if broadcaster is provided
    if let Some(broadcaster) = event_broadcaster {
        let sse_routes: Router<()> = Router::new()
            .route("/events", get(sse_handler))
            .layer(Extension(broadcaster));
        app = app.merge(sse_routes);
    }

    // Merge extra routes if provided (e.g., IAC routes from CLI)
    if let Some(extra) = extra_routes {
        app = app.merge(extra);
    }

    app.layer(cors_layer)
}

/// Start the combined API server on the specified port
pub async fn start_server(
    catalog_state: CatalogState,
    payment_api_config: PaymentApiConfig,
    event_broadcaster: Option<Arc<StatefulEventBroadcaster>>,
    extra_routes: Option<Router<()>>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_combined_router(
        catalog_state,
        payment_api_config,
        event_broadcaster,
        extra_routes,
    );

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
