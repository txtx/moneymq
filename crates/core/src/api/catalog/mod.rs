use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    Extension, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use moneymq_studio_ui::serve_studio_static_files;
use moneymq_types::{Meter, Product, x402::transactions::FacilitatedTransaction};
use stripe::types::{StripeCheckoutSession, StripePaymentIntent};
use url::Url;

use crate::api::sandbox::NetworksConfig;

pub mod db;
pub mod middleware;
pub mod stripe;

use middleware::{x402_get, x402_post};

/// Application state
#[derive(Clone)]
pub struct CatalogState {
    pub facilitator_url: Url,
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub payment_intents: Arc<Mutex<HashMap<String, StripePaymentIntent>>>,
    pub checkout_sessions: Arc<Mutex<HashMap<String, StripeCheckoutSession>>>,
    pub transactions: Arc<Mutex<Vec<FacilitatedTransaction>>>,
    pub networks_config: NetworksConfig,
    pub catalog_name: Option<String>,
    pub catalog_description: Option<String>,
    pub manifest_path: PathBuf,
    pub catalog_path: PathBuf,
    pub use_sandbox: bool,
}

/// Application state
#[derive(Clone)]
pub struct Facilitator {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
}

impl CatalogState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        products: Vec<Product>,
        meters: Vec<Meter>,
        use_sandbox: bool,
        facilitator_url: Url,
        networks_config: NetworksConfig,
        catalog_path: PathBuf,
        catalog_name: Option<String>,
        catalog_description: Option<String>,
        manifest_path: PathBuf,
    ) -> Self {
        Self {
            products: Arc::new(products),
            meters: Arc::new(meters),
            use_sandbox,
            facilitator_url,
            networks_config,
            catalog_path,
            catalog_name,
            catalog_description,
            transactions: Arc::new(Mutex::new(Vec::new())),
            payment_intents: Arc::new(Mutex::new(HashMap::new())),
            checkout_sessions: Arc::new(Mutex::new(HashMap::new())),
            manifest_path,
        }
    }
}

/// Create catalog routes without state layer.
///
/// This is useful when state is injected by middleware per-request (e.g., multi-tenant setups).
/// The routes expect `Extension<CatalogState>` to be present in the request.
///
/// # Example
/// ```ignore
/// let routes = catalog::create_routes();
/// let app = Router::new()
///     .nest("/catalog/v1", routes.clone())
///     .nest("/catalog/v1/sandbox", routes)
///     .layer(middleware_that_injects_state);
/// ```
pub fn create_routes() -> Router<()> {
    Router::new()
        // Product endpoints
        .route("/products", get(stripe::list_products))
        .route(
            "/products/{id}/access",
            x402_get(stripe::get_product_access, None),
        )
        .route("/prices", get(stripe::list_prices))
        // Billing endpoints
        .route("/billing/meters", get(stripe::list_meters))
        .route(
            "/billing/meter_events",
            x402_post(stripe::create_meter_event, None),
        )
        // Customer endpoints
        .route("/customers", post(stripe::create_customer))
        .route("/customers/{id}", post(stripe::update_customer))
        // Payment method endpoints
        .route("/payment_methods", post(stripe::create_payment_method))
        .route(
            "/payment_methods/{id}/attach",
            post(stripe::attach_payment_method),
        )
        // Payment intent endpoints
        .route("/payment_intents", post(stripe::create_payment_intent))
        .route(
            "/payment_intents/{id}",
            get(stripe::retrieve_payment_intent),
        )
        .route(
            "/payment_intents/{id}/confirm",
            x402_post(stripe::confirm_payment_intent, None),
        )
        .route(
            "/payment_intents/{id}/cancel",
            post(stripe::cancel_payment_intent),
        )
        // Subscription endpoints
        .route(
            "/subscriptions",
            x402_post(stripe::create_subscription, None),
        )
        // Checkout session endpoints (Stripe Checkout API)
        .route("/checkout/sessions", post(stripe::create_checkout_session))
        .route(
            "/checkout/sessions/{id}",
            get(stripe::retrieve_checkout_session),
        )
        .route(
            "/checkout/sessions/{id}/line_items",
            get(stripe::list_checkout_session_line_items),
        )
        .route(
            "/checkout/sessions/{id}/expire",
            post(stripe::expire_checkout_session),
        )
}

/// Create the catalog router with all catalog-related routes and state.
///
/// This version applies the state as an Extension layer and is suitable for
/// single-tenant or standalone deployments.
pub fn create_router(state: CatalogState) -> Router<()> {
    create_routes().layer(Extension(state))
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Create the root router with health check and studio fallback
pub fn create_root_router() -> Router<()> {
    Router::new()
        .route("/health", get(health_check))
        .fallback(get(serve_studio_static_files))
}
