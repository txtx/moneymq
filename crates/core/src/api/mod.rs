pub mod catalog;
pub mod payment;
pub mod sandbox;

use axum::{Router, extract::FromRef, routing::get};
use catalog::CatalogState;
use payment::PaymentApiConfig;
// Re-export commonly used types
pub use sandbox::{NetworksConfig, NetworksConfigError};
use tower_http::cors::{Any, CorsLayer};

/// Combined state for routes that need both catalog and payment config
#[derive(Clone)]
pub struct AppState {
    pub catalog: CatalogState,
    pub payment: PaymentApiConfig,
}

impl FromRef<AppState> for CatalogState {
    fn from_ref(state: &AppState) -> Self {
        state.catalog.clone()
    }
}

impl FromRef<AppState> for PaymentApiConfig {
    fn from_ref(state: &AppState) -> Self {
        state.payment.clone()
    }
}

/// Create a combined router that includes both catalog and payment APIs
///
/// # Arguments
/// * `catalog_state` - State for catalog endpoints
/// * `payment_api_config` - Optional state for payment/facilitator endpoints
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

    // Create combined state for routes needing both
    let app_state = AppState {
        catalog: catalog_state.clone(),
        payment: payment_api_config.clone(),
    };

    // Create the catalog router (already includes .with_state())
    let catalog_router = catalog::create_router(catalog_state.clone());

    // Create root-level routes that need both CatalogState and PaymentApiConfig
    let root_routes_with_state: Router<()> = Router::new()
        .route("/config", get(sandbox::config::get_config))
        .route("/sandbox/accounts", get(sandbox::list_accounts))
        .with_state(app_state);

    // Start with root-level routes (health, fallback) and merge stateful routes
    let mut app = catalog::create_root_router()
        .merge(root_routes_with_state)
        .nest("/catalog/v1", catalog_router);

    // Mount the payment/facilitator API under /payment/v1 if provided
    let payment_router = payment::create_router(payment_api_config);
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
