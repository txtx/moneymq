use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

/// MoneyMQ manifest file (Money.toml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Multiple provider configurations
    /// Key is the provider name (e.g., "stripe", "stripe_sandbox")
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (e.g., "stripe")
    pub provider_type: String,

    /// Stripe-specific configuration
    #[serde(flatten)]
    pub stripe_config: StripeConfig,
}

/// Stripe provider configuration
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

    /// Whether to use test mode (default: true)
    #[serde(default = "default_test_mode")]
    pub test_mode: bool,

    /// Reference to a sandbox/test provider configuration
    /// When --sandbox flag is used, this provider will be used instead
    /// Example: sandbox = "stripe_sandbox"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,

    /// Webhook endpoint URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_endpoint: Option<String>,

    /// Webhook secret for signature verification (should be in .env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret_env: Option<String>,
}

fn default_test_mode() -> bool {
    true
}

impl Manifest {
    /// Load manifest from the specified Money.toml file path
    pub fn load(manifest_file_path: &Path) -> Result<Self, String> {
        if !manifest_file_path.exists() {
            return Err(format!(
                "Money.toml not found at {}. Please create a Money.toml file in your project root.",
                manifest_file_path.display()
            ));
        }

        let content = fs::read_to_string(manifest_file_path)
            .map_err(|e| format!("Failed to read {}: {}", manifest_file_path.display(), e))?;

        let manifest: Manifest = toml::from_str(&content)
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
