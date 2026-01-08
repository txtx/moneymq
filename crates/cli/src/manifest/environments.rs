//! Environment configuration types for MoneyMQ manifest.
//!
//! This module defines the deployment environment configurations that determine
//! how MoneyMQ runs in different contexts: local development (Sandbox),
//! self-hosted production (SelfHosted), or cloud hosting (CloudHosted).
//!
//! # Example
//!
//! ```yaml
//! environments:
//!   sandbox:
//!     deployment: Sandbox
//!     binding_address: 0.0.0.0
//!     port: 8488
//!     facilitator:
//!       fee: 0
//!       key_management: TurnKey
//!     network:
//!       chain: Solana
//!       rpc_port: 8899
//!       ws_port: 8900
//!
//!   production:
//!     deployment: CloudHosted
//!     workspace: my-workspace
//!     facilitator:
//!       fee: 0
//!       key_management: TurnKey
//! ```

use indexmap::IndexMap;
use moneymq_types::x402::config::constants::{
    DEFAULT_BINDING_ADDRESS, DEFAULT_MONEYMQ_PORT, DEFAULT_SOLANA_RPC_PORT, DEFAULT_SOLANA_WS_PORT,
};
use serde::{Deserialize, Serialize};

/// Environment configuration for a MoneyMQ deployment.
///
/// Each environment defines how MoneyMQ should run, including:
/// - Server binding address and port
/// - Facilitator settings (fee, key management)
/// - Network configuration (chain, RPC endpoints)
///
/// The `deployment` field in YAML determines which variant is used.
///
/// # Variants
///
/// - [`Sandbox`](SandboxEnvironment) - Local development with embedded Solana validator
/// - [`SelfHosted`](SelfHostedEnvironment) - Production deployment with external RPC
/// - [`CloudHosted`](CloudHostedEnvironment) - Hosted by moneymq.co
///
/// # Example
///
/// ```yaml
/// environments:
///   sandbox:
///     deployment: Sandbox
///     port: 8488
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "deployment")]
pub enum EnvironmentConfig {
    /// Local sandbox environment with an embedded Solana validator.
    ///
    /// This is the default for local development. It automatically starts
    /// a local Surfpool validator and funds test accounts.
    Sandbox(SandboxEnvironment),

    /// Self-hosted environment connecting to an external RPC.
    ///
    /// Use this for production deployments where you manage your own
    /// infrastructure and connect to a Solana RPC provider.
    SelfHosted(SelfHostedEnvironment),

    /// Cloud-hosted environment by moneymq.co.
    ///
    /// Use this to delegate infrastructure management to MoneyMQ's
    /// hosted service. Requires a workspace name.
    CloudHosted(CloudHostedEnvironment),
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        EnvironmentConfig::Sandbox(SandboxEnvironment::default())
    }
}

/// Key management strategy for the facilitator.
///
/// Determines how private keys are stored and managed for signing
/// payment transactions.
///
/// # Variants
///
/// - `InMemory` - Keys generated and held in memory (default, for sandbox)
/// - `TurnKey` - MoneyMQ manages keys automatically (for production)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum KeyManagement {
    /// Keys are generated and held in memory.
    ///
    /// This is the default for sandbox mode. A deterministic keypair
    /// is generated from a fixed seed, ensuring consistent addresses
    /// across restarts during development.
    #[default]
    InMemory,

    /// MoneyMQ manages keys automatically.
    ///
    /// Use this for production deployments where keys are securely
    /// managed by the facilitator service.
    TurnKey,
}

/// Facilitator configuration shared across all deployment types.
///
/// The facilitator is responsible for verifying and settling x402 payments.
///
/// # Example
///
/// ```yaml
/// # Sandbox (InMemory is default, can be omitted)
/// facilitator:
///   fee: 0
///
/// # Production
/// facilitator:
///   fee: 0
///   key_management: TurnKey
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FacilitatorEnvConfig {
    /// Fee charged by the facilitator, in basis points (1/100th of a percent).
    ///
    /// - `0` = no fee (default)
    /// - `100` = 1% fee
    /// - `50` = 0.5% fee
    #[serde(default)]
    pub fee: u64,

    /// Key management strategy for signing transactions.
    ///
    /// Defaults to `InMemory` for sandbox (keys held in memory).
    /// Use `TurnKey` for production deployments.
    #[serde(default, skip_serializing_if = "is_default_key_management")]
    pub key_management: KeyManagement,
}

/// Blockchain network identifier.
///
/// Currently only Solana is supported.
///
/// # Example
///
/// ```yaml
/// network:
///   chain: Solana
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Chain {
    /// Solana blockchain network.
    #[default]
    Solana,
}

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Chain::Solana => write!(f, "Solana"),
        }
    }
}

/// Sandbox environment configuration.
///
/// Runs MoneyMQ with an embedded Solana validator (Surfpool) for local
/// development and testing. Test accounts are automatically funded.
///
/// # Defaults
///
/// - `binding_address`: `0.0.0.0`
/// - `port`: `8488`
/// - `network.rpc_port`: `8899`
/// - `network.ws_port`: `8900`
/// - `jwt_secret`: `moneymq-sandbox-secret` (for local development)
///
/// # Example
///
/// ```yaml
/// sandbox:
///   deployment: Sandbox
///   binding_address: 0.0.0.0
///   port: 8488
///   jwt_secret: my-custom-secret  # optional, has default
///   facilitator:
///     fee: 0
///     key_management: TurnKey
///   network:
///     chain: Solana
///     binding_address: 0.0.0.0
///     rpc_port: 8899
///     ws_port: 8900
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEnvironment {
    /// IP address to bind the MoneyMQ API server.
    ///
    /// Defaults to `0.0.0.0` to accept connections from any interface.
    #[serde(default = "default_binding_address")]
    pub binding_address: String,

    /// Port for the MoneyMQ API server.
    ///
    /// Defaults to `8488`. The API serves both catalog (`/catalog/v1/`)
    /// and payment (`/payment/v1/`) endpoints.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Secret key for signing JWT payment receipts.
    ///
    /// Defaults to `moneymq-sandbox-secret` for local development.
    /// In production, use a secure random secret.
    #[serde(default = "default_sandbox_jwt_secret")]
    pub jwt_secret: String,

    /// Facilitator settings for payment processing.
    #[serde(default)]
    pub facilitator: FacilitatorEnvConfig,

    /// Network configuration for the embedded validator.
    #[serde(default)]
    pub network: SandboxNetworkConfig,
}

impl Default for SandboxEnvironment {
    fn default() -> Self {
        Self {
            binding_address: DEFAULT_BINDING_ADDRESS.to_string(),
            port: DEFAULT_MONEYMQ_PORT,
            jwt_secret: default_sandbox_jwt_secret(),
            facilitator: FacilitatorEnvConfig::default(),
            network: SandboxNetworkConfig::default(),
        }
    }
}

/// Network configuration for sandbox deployment.
///
/// Configures the embedded Solana validator (Surfpool) that runs
/// locally during development.
///
/// # Example
///
/// ```yaml
/// network:
///   chain: Solana
///   recipient: HNohduvBpF...  # optional
///   binding_address: 0.0.0.0
///   rpc_port: 8899
///   ws_port: 8900
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNetworkConfig {
    /// Blockchain network to use.
    ///
    /// Defaults to Solana.
    #[serde(default)]
    pub chain: Chain,

    /// Payment recipient address.
    ///
    /// If not specified, a deterministic address is generated for testing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// IP address to bind the validator RPC and WebSocket servers.
    ///
    /// Defaults to `0.0.0.0`.
    #[serde(default = "default_binding_address")]
    pub binding_address: String,

    /// RPC port for the Solana validator.
    ///
    /// Defaults to `8899`.
    #[serde(default = "default_rpc_port")]
    pub rpc_port: u16,

    /// WebSocket port for the Solana validator.
    ///
    /// Defaults to `8900`. Used for account subscriptions and real-time updates.
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
}

impl Default for SandboxNetworkConfig {
    fn default() -> Self {
        Self {
            chain: Chain::default(),
            recipient: None,
            binding_address: DEFAULT_BINDING_ADDRESS.to_string(),
            rpc_port: DEFAULT_SOLANA_RPC_PORT,
            ws_port: DEFAULT_SOLANA_WS_PORT,
        }
    }
}

/// Self-hosted environment configuration.
///
/// Runs MoneyMQ connecting to an external Solana RPC provider.
/// Use this for production deployments where you manage your own infrastructure.
///
/// # Example
///
/// ```yaml
/// production:
///   deployment: SelfHosted
///   binding_address: 0.0.0.0
///   port: 8488
///   facilitator:
///     fee: 0
///     key_management: TurnKey
///   network:
///     chain: Solana
///     recipient: HNohduvBpF...
///     rpc_url: https://api.mainnet-beta.solana.com
///     ws_url: wss://api.mainnet-beta.solana.com
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHostedEnvironment {
    /// IP address to bind the MoneyMQ API server.
    ///
    /// Defaults to `0.0.0.0`.
    #[serde(default = "default_binding_address")]
    pub binding_address: String,

    /// Port for the MoneyMQ API server.
    ///
    /// Defaults to `8488`.
    #[serde(default = "default_api_port")]
    pub port: u16,

    /// Facilitator settings for payment processing.
    #[serde(default)]
    pub facilitator: FacilitatorEnvConfig,

    /// Network configuration for connecting to external RPC.
    pub network: SelfHostedNetworkConfig,
}

/// Network configuration for self-hosted deployment.
///
/// Connects to an external Solana RPC provider rather than
/// running an embedded validator.
///
/// # Example
///
/// ```yaml
/// network:
///   chain: Solana
///   recipient: HNohduvBpF...
///   rpc_url: https://api.mainnet-beta.solana.com
///   ws_url: wss://api.mainnet-beta.solana.com
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHostedNetworkConfig {
    /// Blockchain network to use.
    ///
    /// Defaults to Solana.
    #[serde(default)]
    pub chain: Chain,

    /// Payment recipient address.
    ///
    /// The Solana address that will receive payments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// RPC URL for the Solana network.
    ///
    /// Examples:
    /// - Mainnet: `https://api.mainnet-beta.solana.com`
    /// - Devnet: `https://api.devnet.solana.com`
    /// - Custom: `https://your-rpc-provider.com`
    pub rpc_url: String,

    /// WebSocket URL for real-time subscriptions.
    ///
    /// If not specified, derived from `rpc_url` by replacing
    /// `https://` with `wss://`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
}

/// Cloud-hosted environment configuration.
///
/// Delegates infrastructure management to MoneyMQ's hosted service.
/// Requires a project name and workspace that you create on moneymq.co.
///
/// # Example
///
/// ```yaml
/// production:
///   deployment: CloudHosted
///   project: My Project
///   workspace: my-company
///   facilitator:
///     fee: 0
///     key_management: TurnKey
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudHostedEnvironment {
    /// Project name for this deployment.
    pub project: String,

    /// Workspace name on moneymq.co.
    ///
    /// Create a workspace at https://moneymq.co to get started.
    pub workspace: String,

    /// Facilitator settings for payment processing.
    #[serde(default)]
    pub facilitator: FacilitatorEnvConfig,
}

// Default value functions for serde
fn default_binding_address() -> String {
    DEFAULT_BINDING_ADDRESS.to_string()
}

fn is_default_key_management(km: &KeyManagement) -> bool {
    matches!(km, KeyManagement::InMemory)
}

fn default_api_port() -> u16 {
    DEFAULT_MONEYMQ_PORT
}

fn default_rpc_port() -> u16 {
    DEFAULT_SOLANA_RPC_PORT
}

fn default_ws_port() -> u16 {
    DEFAULT_SOLANA_WS_PORT
}

fn default_sandbox_jwt_secret() -> String {
    "moneymq-sandbox-secret".to_string()
}

/// Creates a default environments map containing only a sandbox environment.
///
/// This is used when no environments are specified in the manifest.
pub fn default_environments() -> IndexMap<String, EnvironmentConfig> {
    let mut envs = IndexMap::new();
    envs.insert("sandbox".to_string(), EnvironmentConfig::default());
    envs
}
