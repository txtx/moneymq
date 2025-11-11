pub mod db;
pub mod endpoints;
pub mod networks;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
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
use moneymq_types::x402::config::facilitator::{FacilitatorConfig, FacilitatorNetworkConfig};
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};

use crate::facilitator::db::DbManager;

pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";
pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SPL_TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: &str =
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Shared state for the facilitator
#[derive(Clone)]
pub struct FacilitatorState {
    pub config: Arc<FacilitatorConfig>,
    pub db_manager: Arc<db::DbManager>,
    pub kora_config: Arc<Config>,
    pub signer_pool: Arc<SignerPool>,
}

impl FacilitatorState {
    pub fn new(
        config: FacilitatorConfig,
        database_url: &str,
        kora_config: Config,
        signer_pool: SignerPool,
    ) -> Self {
        Self {
            config: Arc::new(config),
            db_manager: Arc::new(DbManager::new(database_url).unwrap()),
            kora_config: Arc::new(kora_config),
            signer_pool: Arc::new(signer_pool),
        }
    }
}

/// Create the facilitator router
pub fn create_router(state: FacilitatorState) -> Router {
    let cors_layer = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/health", get(endpoints::health::handler))
        .route("/verify", post(endpoints::verify::handler))
        .route("/settle", post(endpoints::settle::handler))
        .route("/supported", get(endpoints::supported::handler))
        .route(
            "/v1/transactions",
            get(endpoints::transactions::list_transactions),
        )
        .layer(cors_layer)
        .with_state(state)
}

/// Start the facilitator server
pub async fn start_facilitator(
    config: FacilitatorConfig,
) -> Result<
    JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    Box<dyn std::error::Error>,
> {
    let url = config.url.clone();

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
            FacilitatorNetworkConfig::SolanaSurfnet(cfg) => {
                let key = format!("FACILITATOR_{}_SIGNER_POOL", n);
                unsafe {
                    let value = cfg.payer_keypair.to_base58_string();
                    std::env::set_var(key.clone(), value);
                }
                Some(SignerConfig {
                    name: format!("facilitator-{}-signer", n),
                    weight: None,
                    config: SignerTypeConfig::Memory {
                        config: MemorySignerConfig {
                            private_key_env: key,
                        },
                    },
                })
            }
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

    let state = FacilitatorState::new(
        config,
        format!("sqlite://{}", ":memory:").as_str(),
        kora_config,
        signer_pool,
    );
    let app = create_router(state);

    let addr = format!("0.0.0.0:{}", url.port().expect("URL must have a port"));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to facilitator URL {}: {}", url, e))?;

    let handle =
        tokio::spawn(async move { axum::serve(listener, app).await.map_err(|e| e.into()) });
    Ok(handle)
}
