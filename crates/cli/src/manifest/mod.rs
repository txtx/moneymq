use std::{collections::HashMap, fs, path::Path};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::manifest::x402::X402Config;
pub mod x402;
// TODO: Re-enable x402_rs imports when refactoring X402 facilitator
// use x402_rs::{
//     chain::{NetworkProvider, solana::SolanaProvider},
//     provider_cache::ProviderCache,
// };

/// MoneyMQ manifest file (moneymq.yaml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Multiple provider configurations
    /// Key is the provider name (e.g., "stripe", "stripe_sandbox")
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

impl Manifest {
    /// Load manifest from the specified moneymq.yaml file path
    pub fn load(manifest_file_path: &Path) -> Result<Self, String> {
        if !manifest_file_path.exists() {
            return Err(format!(
                "moneymq.yaml not found at {}. Please create a moneymq.yaml file in your project root.",
                manifest_file_path.display()
            ));
        }

        let content = fs::read_to_string(manifest_file_path)
            .map_err(|e| format!("Failed to read {}: {}", manifest_file_path.display(), e))?;

        let manifest: Manifest = serde_yml::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", manifest_file_path.display(), e))?;

        Ok(manifest)
    }

    /// Try to load manifest, returning a default instance if the file doesn't exist
    pub fn load_or_default(manifest_file_path: &Path) -> Self {
        Self::load(manifest_file_path).unwrap_or_default()
    }

    /// Get a provider configuration by name
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider_type", rename_all = "snake_case")]
pub enum ProviderConfig {
    Stripe(StripeConfig),
    #[serde(rename = "x402")]
    X402(X402Config),
}

impl std::fmt::Display for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderConfig::Stripe(_) => write!(f, "Stripe"),
            ProviderConfig::X402(_) => write!(f, "X402"),
        }
    }
}

impl ProviderConfig {
    /// Get Stripe configuration if this is a Stripe provider
    pub fn stripe_config(&self) -> Option<&StripeConfig> {
        match self {
            ProviderConfig::Stripe(config) => Some(config),
            ProviderConfig::X402(_) => None,
        }
    }
    /// Get X402 configuration if this is an X402 provider
    pub fn x402_config(&self) -> Option<&X402Config> {
        match self {
            ProviderConfig::X402(config) => Some(config),
            ProviderConfig::Stripe(_) => None,
        }
    }
}

/// Stripe sandbox/test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSandboxConfig {
    /// Optional description of this sandbox environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Stripe API secret key (optional)
    /// WARNING: It's recommended to use STRIPE_SANDBOX_SECRET_KEY environment variable instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// API version to use (optional, defaults to Stripe's latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// Catalog path (e.g., "billing/catalog/v1") - defaults to "billing/catalog/v1"
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,

    /// Webhook endpoint URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_endpoint: Option<String>,

    /// Webhook secret for signature verification (should be in .env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret_env: Option<String>,
}

impl Default for StripeSandboxConfig {
    fn default() -> Self {
        Self {
            description: None,
            api_key: None,
            api_version: None,
            catalog_path: default_catalog_path(),
            webhook_endpoint: None,
            webhook_secret_env: None,
        }
    }
}

impl StripeSandboxConfig {
    pub fn api_key(&self) -> Option<&String> {
        self.api_key.as_ref()
    }

    pub fn api_version(&self) -> Option<&String> {
        self.api_version.as_ref()
    }
}

/// Stripe provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeConfig {
    /// Optional description of this provider (e.g., "Stripe account - Acme Corp")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Stripe API secret key (optional)
    /// WARNING: It's recommended to use STRIPE_SECRET_KEY environment variable instead
    /// to avoid committing secrets to version control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// API version to use (optional, defaults to Stripe's latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    /// Catalog path (e.g., "billing/catalog/v1") - defaults to "billing/catalog/v1"
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,

    /// Whether to use test mode (default: true)
    #[serde(default = "default_test_mode")]
    pub test_mode: bool,

    /// Webhook endpoint URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_endpoint: Option<String>,

    /// Webhook secret for signature verification (should be in .env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret_env: Option<String>,

    /// Nested sandbox/test configurations
    /// Key is the sandbox name (e.g., "default", "staging", "test")
    /// When --sandbox flag is used, the "default" sandbox will be used
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, StripeSandboxConfig>,
}

impl Default for StripeConfig {
    fn default() -> Self {
        Self {
            description: None,
            api_key: None,
            api_version: None,
            catalog_path: default_catalog_path(),
            test_mode: default_test_mode(),
            webhook_endpoint: None,
            webhook_secret_env: None,
            sandboxes: IndexMap::new(),
        }
    }
}

fn default_test_mode() -> bool {
    true
}

fn default_catalog_path() -> String {
    "billing/catalog/v1".to_string()
}
