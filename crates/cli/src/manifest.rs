use std::{collections::HashMap, fs, path::Path};

use moneymq_core::facilitator::FacilitatorConfig;
use serde::{Deserialize, Serialize};
use solana_keypair::{EncodableKey, Keypair};
use x402_rs::{
    chain::{NetworkProvider, solana::SolanaProvider},
    network::Network,
    provider_cache::ProviderCache,
};

/// MoneyMQ manifest file (Money.toml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Multiple provider configurations
    /// Key is the provider name (e.g., "stripe", "stripe_sandbox")
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
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

/// x402 provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X402Config {
    /// Whether to use test mode (default: true)
    #[serde(default = "default_test_mode")]
    pub test_mode: bool,

    /// Reference to a sandbox/test provider configuration
    /// When --sandbox flag is used, this provider will be used instead
    /// Example: sandbox = "x402_sandbox"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,

    /// Facilitator service URL
    pub facilitator_url: Option<String>,

    /// Configuration for local facilitator
    pub local_facilitator: Option<FacilitatorConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FacilitatorConfigFile {
    /// Facilitator service host (e.g., "localhost")
    pub host: String,
    /// Facilitator service port (e.g., 8080)
    pub port: u16,
    /// Facilitator provider configurations
    pub providers: HashMap<String, FacilitatorProviderConfigFile>,
}

impl TryInto<FacilitatorConfig> for FacilitatorConfigFile {
    type Error = String;

    fn try_into(self) -> Result<FacilitatorConfig, Self::Error> {
        let providers = self
            .providers
            .iter()
            .map(|(_, config)| config.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let provider_cache = if providers.is_empty() {
            None
        } else {
            Some(ProviderCache::from_iter(providers.into_iter()))
        };
        Ok(FacilitatorConfig {
            host: self.host.clone(),
            port: self.port,
            provider_cache,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "chain_type", rename_all = "snake_case")]
pub enum FacilitatorProviderConfigFile {
    Solana(SolanaFacilitatorProviderConfigFile),
    Evm(EvmFacilitatorProviderConfig),
}

impl TryInto<(Network, NetworkProvider)> for &FacilitatorProviderConfigFile {
    type Error = String;

    fn try_into(self) -> Result<(Network, NetworkProvider), Self::Error> {
        match self {
            FacilitatorProviderConfigFile::Evm(_) => {
                todo!("Implement EVM provider configuration")
            }
            FacilitatorProviderConfigFile::Solana(config) => {
                let network = config.network;
                let provider: NetworkProvider = config.try_into()?;
                Ok((network, provider))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaFacilitatorProviderConfigFile {
    pub network: Network,
    pub keypair_path: Option<String>,
    pub rpc_url: String,
}

impl TryInto<NetworkProvider> for &SolanaFacilitatorProviderConfigFile {
    type Error = String;
    fn try_into(self) -> Result<NetworkProvider, Self::Error> {
        let network = self.network;
        let keypair = if let Some(ref path) = self.keypair_path {
            Keypair::read_from_file(path)
                .map_err(|e| format!("Failed to read Solana keypair from file: {}", e))?
        } else {
            Keypair::new()
        };

        let provider = SolanaProvider::try_new(keypair, self.rpc_url.clone(), network)
            .expect("Failed to create Solana provider");
        Ok(NetworkProvider::Solana(provider))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmFacilitatorProviderConfig {
    pub network: Network,
    pub secret_key_path: Option<String>,
    pub rpc_url: Option<String>,
}

fn default_test_mode() -> bool {
    true
}
