pub mod catalog;
pub mod payment;
pub mod sandbox;

use axum::{Router, routing::get};
use catalog::ProviderState;
use payment::FacilitatorState;
// Re-export commonly used types
pub use sandbox::{NetworksConfig, NetworksConfigError};
use tower_http::cors::{Any, CorsLayer};

/// Create a combined router that includes both catalog and payment APIs
pub fn create_combined_router(
    catalog_state: ProviderState,
    facilitator_state: Option<FacilitatorState>,
) -> Router<()> {
    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create the catalog router (already includes .with_state())
    let catalog_router = catalog::create_router(catalog_state.clone());

    // Create root-level routes that need ProviderState
    let root_routes_with_state: Router<()> = Router::new()
        .route("/config", get(catalog::config::get_config))
        .route("/sandbox/accounts", get(sandbox::list_accounts))
        .with_state(catalog_state);

    // Start with root-level routes (health, fallback) and merge stateful routes
    let mut app = catalog::create_root_router()
        .merge(root_routes_with_state)
        .nest("/catalog/v1", catalog_router);

    // Mount the payment/facilitator API under /payment/v1 if provided
    if let Some(facilitator_state) = facilitator_state {
        let payment_router = payment::create_router(facilitator_state);
        app = app.nest("/payment/v1", payment_router);
    }

    app.layer(cors_layer)
}

/// Start the combined API server on the specified port
pub async fn start_server(
    catalog_state: ProviderState,
    facilitator_state: Option<FacilitatorState>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_combined_router(catalog_state, facilitator_state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
