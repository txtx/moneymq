use std::{fs, path::Path};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::manifest::x402::X402Config;
pub mod x402;
// TODO: Re-enable x402_rs imports when refactoring X402 facilitator
// use x402_rs::{
//     chain::{NetworkProvider, solana::SolanaProvider},
//     provider_cache::ProviderCache,
// };

/// MoneyMQ manifest file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Multiple catalog configurations
    /// Key is the catalog name (e.g., "stripe", "stripe_sandbox")
    #[serde(default)]
    pub catalogs: IndexMap<String, CatalogConfig>,
    #[serde(default)]
    pub networks: IndexMap<String, X402Config>,
}

impl Manifest {
    /// Load manifest from the specified file path
    pub fn load(manifest_file_path: &Path) -> Result<Self, String> {
        if !manifest_file_path.exists() {
            return Err(format!(
                "{} not found at {}. Please create a {} file in your project root.",
                moneymq_types::MANIFEST_FILE_NAME,
                manifest_file_path.display(),
                moneymq_types::MANIFEST_FILE_NAME
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

    /// Get a catalog configuration by name
    pub fn get_catalog(&self, name: &str) -> Option<&CatalogConfig> {
        self.catalogs.get(name)
    }

    /// Get a network configuration by name
    pub fn get_network(&self, name: &str) -> Option<&X402Config> {
        self.networks.get(name)
    }
}

/// Catalog configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    /// Optional description of this catalog (e.g., "Stripe account - Acme Corp")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Catalog path - base path for billing data (e.g., "billing/v1") - defaults to "billing/v1"
    /// Products are in {catalog_path}/products, meters in {catalog_path}/meters, etc.
    #[serde(default = "default_catalog_path")]
    pub catalog_path: String,

    /// The source/provider for this catalog (defaults to Stripe if not specified)
    #[serde(flatten, default = "default_catalog_source")]
    pub source: CatalogSourceType,
}

impl CatalogConfig {
    /// Get Stripe configuration if this catalog uses Stripe as source
    pub fn stripe_config(&self) -> Option<&StripeConfig> {
        match &self.source {
            CatalogSourceType::Stripe(config) => Some(config),
        }
    }
}

#[allow(dead_code)]
fn default_catalog_source() -> CatalogSourceType {
    CatalogSourceType::Stripe(StripeConfig::default())
}

/// Catalog source type (Stripe, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum CatalogSourceType {
    Stripe(StripeConfig),
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

/// Stripe catalog source configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StripeConfig {
    /// Stripe API secret key (optional)
    /// WARNING: It's recommended to use STRIPE_SECRET_KEY environment variable instead
    /// to avoid committing secrets to version control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// API version to use (optional, defaults to Stripe's latest)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

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

fn default_catalog_path() -> String {
    "billing/v1".to_string()
}
