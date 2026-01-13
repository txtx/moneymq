pub mod db;
pub mod endpoints;
pub mod networks;

use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Compute a deterministic channel ID from a Solana transaction.
///
/// This function hashes the transaction message bytes (excluding signatures)
/// to produce a consistent ID that can be computed by both frontend and backend:
/// - Frontend: Hash the unsigned transaction message before signing
/// - Backend: Extract message bytes from signed transaction and hash
///
/// Returns a hex-encoded SHA256 hash that can be used as a channel ID.
pub fn channel_id_from_transaction(transaction_str: &str) -> Result<String, String> {
    let message_bytes = networks::solana::extract_transaction_message_bytes(transaction_str)
        .map_err(|e| format!("Failed to extract transaction message: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(&message_bytes);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

use axum::{
    Extension, Router,
    routing::{get, post},
};
use cloudevents::Event;
use crossbeam_channel::Sender;
use kora_lib::{
    Config,
    config::{FeePayerPolicy, KoraConfig, MetricsConfig, Token2022Config, ValidationConfig},
    fee::price::{PriceConfig, PriceModel},
    oracle::PriceSource,
    signer::{
        MemorySignerConfig, SelectionStrategy, SignerConfig, SignerPool, SignerPoolConfig,
        SignerTypeConfig, config::SignerPoolSettings,
    },
};
use moneymq_types::x402::config::facilitator::{
    FacilitatorConfig, FacilitatorNetworkConfig, ValidatorsConfig,
};
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};

use crate::api::payment::db::DbManager;

pub const SOLANA_KEYPAIR_ENV: &str = "MONEYMQ_SOLANA_FACILITATOR_KEYPAIR";
pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";
pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SPL_TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: &str =
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Shared state for the facilitator
#[derive(Clone)]
pub struct PaymentApiConfig {
    pub facilitator_config: Arc<FacilitatorConfig>,
    pub validators: Arc<ValidatorsConfig>,
    pub db_manager: Arc<db::DbManager>,
    pub kora_config: Arc<Config>,
    pub signer_pool: Arc<SignerPool>,
    pub payment_stack_id: String,
    pub is_sandbox: bool,
    pub event_sender: Option<Sender<Event>>,
    /// Channel manager for pub/sub event streaming
    pub channel_manager: Option<Arc<endpoints::channels::ChannelManager>>,
    /// JWT key pair for signing payment receipts (ES256)
    pub jwt_key_pair: Option<Arc<endpoints::jwt::JwtKeyPair>>,
    /// Payout recipient address (where payments are sent)
    pub payout_recipient_address: Option<String>,
    /// Facilitator address (fee payer for transactions)
    pub facilitator_address: Option<String>,
    /// Stack/merchant name for branding
    pub stack_name: Option<String>,
    /// Stack/merchant image URL for branding
    pub stack_image_url: Option<String>,
    /// Sandbox operator accounts
    pub accounts: Arc<moneymq_types::AccountsConfig>,
}

impl PaymentApiConfig {
    pub fn local(
        facilitator_config: FacilitatorConfig,
        validators: ValidatorsConfig,
        database_url: &str,
        kora_config: Config,
        signer_pool: SignerPool,
    ) -> Self {
        // Extract payment_stack_id from the URL's subdomain
        let payment_stack_id = facilitator_config
            .url
            .host_str()
            .and_then(|host| host.split('.').next())
            .unwrap_or("local")
            .to_string();

        Self {
            facilitator_config: Arc::new(facilitator_config),
            validators: Arc::new(validators),
            db_manager: Arc::new(DbManager::local(database_url).unwrap()),
            kora_config: Arc::new(kora_config),
            signer_pool: Arc::new(signer_pool),
            payment_stack_id,
            is_sandbox: true, // Local mode defaults to sandbox
            event_sender: None,
            channel_manager: None,
            jwt_key_pair: None,
            payout_recipient_address: None,
            facilitator_address: None,
            stack_name: None,
            stack_image_url: None,
            accounts: Arc::new(indexmap::IndexMap::new()),
        }
    }

    pub fn new(
        facilitator_config: FacilitatorConfig,
        validators: ValidatorsConfig,
        db_manager: db::DbManager,
        kora_config: Config,
        signer_pool: SignerPool,
        payment_stack_id: String,
        is_sandbox: bool,
    ) -> Self {
        Self {
            facilitator_config: Arc::new(facilitator_config),
            validators: Arc::new(validators),
            db_manager: Arc::new(db_manager),
            kora_config: Arc::new(kora_config),
            signer_pool: Arc::new(signer_pool),
            payment_stack_id,
            is_sandbox,
            event_sender: None,
            channel_manager: None,
            jwt_key_pair: None,
            payout_recipient_address: None,
            facilitator_address: None,
            stack_name: None,
            stack_image_url: None,
            accounts: Arc::new(indexmap::IndexMap::new()),
        }
    }

    /// Set the event sender for CloudEvents
    pub fn with_event_sender(mut self, sender: Sender<Event>) -> Self {
        self.event_sender = Some(sender);
        self
    }

    /// Set the channel manager for pub/sub event streaming
    pub fn with_channel_manager(
        mut self,
        manager: Arc<endpoints::channels::ChannelManager>,
    ) -> Self {
        self.channel_manager = Some(manager);
        self
    }

    /// Set the JWT key pair for signing payment receipts (ES256)
    pub fn with_jwt_secret(mut self, secret: String) -> Self {
        let key_pair = endpoints::jwt::JwtKeyPair::from_secret(&secret);
        self.jwt_key_pair = Some(Arc::new(key_pair));
        self
    }

    /// Set the payout recipient address
    pub fn with_payout_recipient(mut self, address: String) -> Self {
        self.payout_recipient_address = Some(address);
        self
    }

    /// Set the facilitator address (fee payer)
    pub fn with_facilitator_address(mut self, address: String) -> Self {
        self.facilitator_address = Some(address);
        self
    }

    /// Set the stack/merchant branding
    pub fn with_stack_branding(mut self, name: Option<String>, image_url: Option<String>) -> Self {
        self.stack_name = name;
        self.stack_image_url = image_url;
        self
    }

    /// Set the sandbox accounts
    pub fn with_accounts(mut self, accounts: moneymq_types::AccountsConfig) -> Self {
        self.accounts = Arc::new(accounts);
        self
    }
}

/// JWKS endpoint handler - returns public keys for JWT verification
async fn jwks_handler(
    Extension(state): Extension<PaymentApiConfig>,
) -> axum::response::Json<endpoints::jwt::JwksResponse> {
    let jwks = state
        .jwt_key_pair
        .as_ref()
        .map(|kp| kp.jwks())
        .unwrap_or_else(|| endpoints::jwt::JwksResponse { keys: vec![] });

    axum::response::Json(jwks)
}

/// Create payment routes without state layer.
///
/// This is useful when state is injected by middleware per-request (e.g., multi-tenant setups).
/// The routes expect `Extension<PaymentApiConfig>` to be present in the request.
///
/// # Example
/// ```ignore
/// let routes = payment::create_routes();
/// let app = Router::new()
///     .nest("/payment/v1", routes.clone())
///     .nest("/payment/v1/sandbox", routes)
///     .layer(middleware_that_injects_state);
/// ```
pub fn create_routes() -> Router {
    Router::new()
        .route("/health", get(endpoints::health::handler))
        .route("/config", get(endpoints::config::handler))
        .route("/verify", post(endpoints::verify::handler))
        .route("/settle", post(endpoints::settle::handler))
        .route("/supported", get(endpoints::supported::handler))
        .route("/.well-known/jwks.json", get(jwks_handler))
        .route(
            "/admin/transactions",
            get(endpoints::admin::list_transactions),
        )
        .route("/events", get(endpoints::events::handler))
}

/// Create the facilitator router with state and optional channel manager.
///
/// This version applies the state as an Extension layer and is suitable for
/// single-tenant or standalone deployments.
pub fn create_router(state: PaymentApiConfig) -> Router {
    let cors_layer = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    let channel_manager = state.channel_manager.clone();

    let mut router = create_routes().layer(Extension(state));

    // Add channel routes if manager is configured
    if let Some(manager) = channel_manager {
        let channel_routes = endpoints::channels::create_router(manager);
        router = router.merge(channel_routes);
    }

    router.layer(cors_layer)
}

/// Create a PaymentApiConfig from a FacilitatorConfig without starting a server
pub async fn create_payment_api_config(
    config: FacilitatorConfig,
    validators: ValidatorsConfig,
    _sandbox: bool,
) -> Result<PaymentApiConfig, Box<dyn std::error::Error>> {
    let kora_config = Config {
        validation: ValidationConfig {
            max_allowed_lamports: 100_000_000, // 0.1 SOL
            max_signatures: 10,
            allowed_programs: vec![
                SYSTEM_PROGRAM_ID.to_string(),
                COMPUTE_BUDGET_PROGRAM_ID.to_string(),
                SPL_TOKEN_PROGRAM_ID.to_string(),
                SPL_TOKEN_2022_PROGRAM_ID.to_string(),
                SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID.to_string(),
            ],
            allowed_tokens: vec![],
            allowed_spl_paid_tokens: kora_lib::config::SplTokenConfig::All,
            disallowed_accounts: vec![],
            price_source: PriceSource::Mock,
            fee_payer_policy: FeePayerPolicy::default(),
            price: PriceConfig {
                model: PriceModel::Free,
            },
            token_2022: Token2022Config::default(),
        },
        kora: KoraConfig::default(),
        metrics: MetricsConfig::default(),
    };

    let signers = config
        .networks
        .iter()
        .filter_map(|(n, c)| match c {
            FacilitatorNetworkConfig::SolanaSurfnet(_) => Some(SignerConfig {
                name: format!("facilitator-{}-signer", n),
                weight: None,
                config: SignerTypeConfig::Memory {
                    config: MemorySignerConfig {
                        // Safe to assume the keypair is set here
                        private_key_env: SOLANA_KEYPAIR_ENV.into(),
                    },
                },
            }),
            FacilitatorNetworkConfig::SolanaMainnet(_) => None,
        })
        .collect::<Vec<_>>();

    let signer_pool_config = SignerPoolConfig {
        signers,
        signer_pool: SignerPoolSettings {
            strategy: SelectionStrategy::RoundRobin,
        },
    };
    let signer_pool = SignerPool::from_config(signer_pool_config).await?;

    let state = PaymentApiConfig::local(
        config,
        validators,
        format!("sqlite://{}", "payments.sqlite").as_str(),
        kora_config,
        signer_pool,
    );

    Ok(state)
}

/// Start the facilitator server
pub async fn start_facilitator(
    config: FacilitatorConfig,
    validators: ValidatorsConfig,
    sandbox: bool,
) -> Result<
    JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    Box<dyn std::error::Error>,
> {
    let url = config.url.clone();
    let state = create_payment_api_config(config, validators, sandbox).await?;
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", url.port().expect("URL must have a port"));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to facilitator URL {}: {}", url, e))?;

    let handle =
        tokio::spawn(async move { axum::serve(listener, app).await.map_err(|e| e.into()) });
    Ok(handle)
}
