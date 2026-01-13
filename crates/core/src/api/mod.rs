pub mod catalog;
pub mod payment;
pub mod sandbox;

use axum::{Extension, Router, routing::get};
use catalog::CatalogState;
use payment::PaymentApiConfig;
// Backwards compatibility re-export
#[allow(deprecated)]
pub use sandbox::generate_sandbox_accounts;
// Re-export commonly used types
pub use sandbox::{
    NetworksConfig, NetworksConfigError, SANDBOX_FACILITATOR_SEED, generate_sandbox_actors,
};
use tower_http::cors::{Any, CorsLayer};

/// Create a combined router that includes both catalog and payment APIs
///
/// # Arguments
/// * `catalog_state` - State for catalog endpoints
/// * `payment_api_config` - State for payment/facilitator endpoints
/// * `extra_routes` - Optional additional routes to merge (e.g., IAC routes from CLI)
pub fn create_combined_router(
    catalog_state: CatalogState,
    payment_api_config: PaymentApiConfig,
    extra_routes: Option<Router<()>>,
) -> Router<()> {
    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create the catalog router (uses Extension layer internally)
    let catalog_router = catalog::create_router(catalog_state.clone());

    // Start with root-level routes (health, fallback)
    let mut app = catalog::create_root_router().nest("/catalog/v1", catalog_router);

    // Mount the payment/facilitator API under /payment/v1
    // Include /accounts endpoint (sandbox-only, needs NetworksConfig)
    // SSE events are served via /payment/v1/events using DB polling
    let networks_config = catalog_state.networks_config.clone();
    let accounts_route: Router<()> = Router::new()
        .route("/accounts", get(sandbox::list_accounts))
        .layer(Extension(networks_config))
        .layer(Extension(payment_api_config.clone()));
    let payment_router = payment::create_router(payment_api_config).merge(accounts_route);
    app = app.nest("/payment/v1", payment_router);

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
    extra_routes: Option<Router<()>>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_combined_router(catalog_state, payment_api_config, extra_routes);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
