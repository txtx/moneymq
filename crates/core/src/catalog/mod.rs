pub mod config;
pub mod stripe;

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
use tower_http::cors::{Any, CorsLayer};
use url::Url;

use crate::{billing::NetworksConfig, facilitator::endpoints::middleware::x402_post, gateway};

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
        }
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Start the provider server
#[allow(clippy::too_many_arguments)]
pub async fn start_provider(
    products: Vec<Product>,
    meters: Vec<Meter>,
    facilitator_url: Url,
    port: u16,
    use_sandbox: bool,
    networks_config: NetworksConfig,
    catalog_path: PathBuf,
    catalog_name: Option<String>,
    catalog_description: Option<String>,
    facilitator_pubkey: String,
    validators_config: ValidatorsConfig,
    kora_config: Option<Arc<kora_lib::Config>>,
    signer_pool: Option<Arc<kora_lib::signer::SignerPool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = ProviderState::new(
        products,
        meters,
        use_sandbox,
        facilitator_url,
        networks_config,
        catalog_path,
        catalog_name,
        catalog_description,
        facilitator_pubkey,
        validators_config,
        kora_config,
        signer_pool,
    );

    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Config endpoint
        .route("/config", get(config::get_config))
        // Product endpoints
        .route("/v1/products", get(stripe::list_products))
        .route("/v1/prices", get(stripe::list_prices))
        // Billing endpoints
        .route("/v1/billing/meters", get(stripe::list_meters))
        .route(
            "/v1/billing/meter_events",
            x402_post(stripe::create_meter_event, Some(state.clone())),
        )
        // Customer endpoints
        .route("/v1/customers", post(stripe::create_customer))
        .route("/v1/customers/{id}", post(stripe::update_customer))
        // Payment method endpoints
        .route("/v1/payment_methods", post(stripe::create_payment_method))
        .route(
            "/v1/payment_methods/{id}/attach",
            post(stripe::attach_payment_method),
        )
        // Payment intent endpoints
        .route("/v1/payment_intents", post(stripe::create_payment_intent))
        .route(
            "/v1/payment_intents/{id}",
            get(stripe::retrieve_payment_intent),
        )
        .route(
            "/v1/payment_intents/{id}/confirm",
            x402_post(stripe::confirm_payment_intent, Some(state.clone())),
        )
        .route(
            "/v1/payment_intents/{id}/cancel",
            post(stripe::cancel_payment_intent),
        )
        // Subscription endpoints
        .route(
            "/v1/subscriptions",
            x402_post(stripe::create_subscription, Some(state.clone())),
        )
        // Sandbox dev endpoints
        .route(
            "/sandbox/accounts",
            get(gateway::endpoints::sandbox::list_accounts),
        )
        .fallback(get(serve_studio_static_files))
        .layer(cors_layer)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting MoneyMQ provider server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
