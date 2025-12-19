use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use moneymq_studio_ui::serve_studio_static_files;
use moneymq_types::{
    Meter, Product,
    x402::{config::facilitator::ValidatorsConfig, transactions::FacilitatedTransaction},
};
use stripe::types::StripePaymentIntent;
use url::Url;

use crate::api::{payment::endpoints::middleware::x402_post, sandbox::NetworksConfig};

pub mod config;
pub mod db;
pub mod stripe;

/// Application state
#[derive(Clone)]
pub struct ProviderState {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
    pub use_sandbox: bool,
    pub facilitator_url: Url,
    pub networks_config: NetworksConfig,
    pub catalog_path: PathBuf,
    pub catalog_name: Option<String>,
    pub catalog_description: Option<String>,
    pub facilitator_pubkey: String,
    pub validators: ValidatorsConfig,
    pub transactions: Arc<Mutex<Vec<FacilitatedTransaction>>>,
    pub payment_intents: Arc<Mutex<HashMap<String, StripePaymentIntent>>>,
    pub kora_config: Option<Arc<kora_lib::Config>>,
    pub signer_pool: Option<Arc<kora_lib::signer::SignerPool>>,
    pub manifest_path: PathBuf,
}

/// Application state
#[derive(Clone)]
pub struct Facilitator {
    pub products: Arc<Vec<Product>>,
    pub meters: Arc<Vec<Meter>>,
}

impl ProviderState {
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
        facilitator_pubkey: String,
        validators_config: ValidatorsConfig,
        kora_config: Option<Arc<kora_lib::Config>>,
        signer_pool: Option<Arc<kora_lib::signer::SignerPool>>,
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
            facilitator_pubkey,
            validators: validators_config,
            transactions: Arc::new(Mutex::new(Vec::new())),
            payment_intents: Arc::new(Mutex::new(HashMap::new())),
            kora_config,
            signer_pool,
            manifest_path,
        }
    }
}

/// Create the catalog router with all catalog-related routes
pub fn create_router(state: ProviderState) -> Router<()> {
    Router::new()
        // Product endpoints
        .route("/products", get(stripe::list_products))
        .route("/prices", get(stripe::list_prices))
        // Billing endpoints
        .route("/billing/meters", get(stripe::list_meters))
        .route(
            "/billing/meter_events",
            x402_post(stripe::create_meter_event, Some(state.clone())),
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
            x402_post(stripe::confirm_payment_intent, Some(state.clone())),
        )
        .route(
            "/payment_intents/{id}/cancel",
            post(stripe::cancel_payment_intent),
        )
        // Subscription endpoints
        .route(
            "/subscriptions",
            x402_post(stripe::create_subscription, Some(state.clone())),
        )
        .with_state(state)
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
