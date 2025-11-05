use std::collections::HashMap;

use crate::manifest::default_test_mode;
use indexmap::IndexMap;
use moneymq_types::x402::config::facilitator::{
    FacilitatorConfig, FacilitatorNetworkConfig, SolanaMainnetFacilitatorConfig,
    SolanaSurfnetFacilitatorConfig,
};
use serde::{Deserialize, Serialize};
use solana_keypair::{EncodableKey, Keypair};
use url::Url;

pub const DEFAULT_LOCAL_FACILITATOR_URL: &str = "http://localhost:8080";
pub const DEFAULT_PROD_FACILITATOR_URL: &str = "https://facilitator.moneymq.co";

/// x402 provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X402Config {
    /// Optional description of this provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether to use test mode (default: true)
    #[serde(default = "default_test_mode")]
    pub test_mode: bool,

    /// Configuration of the networks supported by this provider
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub billing_networks: IndexMap<String, BillingNetworkConfigFile>,

    /// Facilitator config
    pub facilitator: FacilitatorConfigFile,

    /// Nested sandbox/test configurations
    /// Key is the sandbox name (e.g., "default", "staging", "test")
    /// When --sandbox flag is used, the "default" sandbox will be used
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, X402SandboxConfig>,
}

/// Configurations for different network types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "network_type", rename_all = "kebab-case")]
pub enum BillingNetworkConfigFile {
    SolanaSurfnet(SolanaSurfnetBillingConfigFile),
    SolanaMainnet(SolanaMainnetBillingConfigFile),
}

impl BillingNetworkConfigFile {
    pub fn network(&self) -> moneymq_types::x402::MoneyMqNetwork {
        match self {
            BillingNetworkConfigFile::SolanaSurfnet(_) => {
                moneymq_types::x402::MoneyMqNetwork::SolanaSurfnet
            }
            BillingNetworkConfigFile::SolanaMainnet(_) => {
                moneymq_types::x402::MoneyMqNetwork::SolanaMainnet
            }
        }
    }
    pub fn payment_recipient(&self) -> Option<String> {
        match self {
            BillingNetworkConfigFile::SolanaSurfnet(cfg) => cfg.payment_recipient.clone(),
            BillingNetworkConfigFile::SolanaMainnet(cfg) => cfg.payment_recipient.clone(),
        }
    }
    pub fn currencies(&self) -> &Vec<String> {
        match self {
            BillingNetworkConfigFile::SolanaSurfnet(cfg) => &cfg.currencies,
            BillingNetworkConfigFile::SolanaMainnet(cfg) => &cfg.currencies,
        }
    }
    pub fn user_accounts(&self) -> Vec<String> {
        match self {
            BillingNetworkConfigFile::SolanaSurfnet(cfg) => cfg.user_accounts.clone(),
            BillingNetworkConfigFile::SolanaMainnet(_) => vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaSurfnetBillingConfigFile {
    pub payment_recipient: Option<String>,
    pub currencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaMainnetBillingConfigFile {
    pub payment_recipient: Option<String>,
    pub currencies: Vec<String>,
}

/// x402 Facilitator configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FacilitatorConfigFile {
    /// The facilitator service URL
    pub url: Option<String>,
    /// Optional API token for authenticating with the facilitator
    pub api_token: Option<String>,
    /// Configuration of the networks supported by the facilitator
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub networks: IndexMap<String, FacilitatorProviderConfigFile>,
}

impl TryInto<FacilitatorConfig> for &FacilitatorConfigFile {
    type Error = String;
    fn try_into(self) -> Result<FacilitatorConfig, Self::Error> {
        let mut networks = HashMap::new();
        for (name, net_config_file) in &self.networks {
            networks.insert(
                name.clone(),
                net_config_file.try_into().map_err(|e| {
                    format!(
                        "Failed to parse facilitator network config for {}: {}",
                        name, e
                    )
                })?,
            );
        }

        let url = if networks.is_empty() {
            // if no network config is provided, we're using an existing facilitator (using ours as default)
            match self.url.as_ref() {
                Some(url_str) => url_str
                    .parse::<Url>()
                    .map_err(|e| format!("Failed to parse facilitator URL {}: {}", url_str, e))?,
                None => DEFAULT_PROD_FACILITATOR_URL
                    .parse::<Url>()
                    .expect("Failed to parse default production URL"),
            }
        } else {
            // if a network config is provided, we're starting a facilitator, so a localhost URL is default
            match self.url.as_ref() {
                Some(url_str) => url_str
                    .parse::<Url>()
                    .map_err(|e| format!("Failed to parse facilitator URL {}: {}", url_str, e))?,
                None => DEFAULT_LOCAL_FACILITATOR_URL
                    .parse::<Url>()
                    .expect("Failed to parse default localhost URL"),
            }
        };

        Ok(FacilitatorConfig {
            url,
            networks,
            api_token: self.api_token.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "network_type", rename_all = "kebab-case")]
pub enum FacilitatorProviderConfigFile {
    SolanaSurfnet(SolanaSurfnetFacilitatorProviderConfigFile),
    SolanaMainnet(SolanaMainnetFacilitatorProviderConfigFile),
}

impl TryInto<FacilitatorNetworkConfig> for &FacilitatorProviderConfigFile {
    type Error = String;
    fn try_into(self) -> Result<FacilitatorNetworkConfig, Self::Error> {
        match self {
            FacilitatorProviderConfigFile::SolanaSurfnet(cfg_file) => Ok(
                FacilitatorNetworkConfig::SolanaSurfnet(cfg_file.try_into().map_err(|e| {
                    format!("Failed to parse Solana Surfnet facilitator config: {}", e)
                })?),
            ),
            FacilitatorProviderConfigFile::SolanaMainnet(cfg_file) => Ok(
                FacilitatorNetworkConfig::SolanaMainnet(cfg_file.try_into().map_err(|e| {
                    format!("Failed to parse Solana Mainnet facilitator config: {}", e)
                })?),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaSurfnetFacilitatorProviderConfigFile {
    pub payer_keypair_path: Option<String>,
    pub rpc_url: Option<String>,
}

impl TryInto<SolanaSurfnetFacilitatorConfig> for &SolanaSurfnetFacilitatorProviderConfigFile {
    type Error = String;
    fn try_into(self) -> Result<SolanaSurfnetFacilitatorConfig, Self::Error> {
        let rpc_url = if let Some(ref url) = self.rpc_url {
            url.parse::<Url>()
                .map_err(|e| format!("Failed to parse RPC URL {}: {}", url, e))?
        } else {
            "http://127.0.0.1:8899"
                .parse::<Url>()
                .expect("Failed to parse default Surfnet RPC URL")
        };

        let payer_keypair = if let Some(ref path) = self.payer_keypair_path {
            Keypair::read_from_file(path)
                .map_err(|e| format!("Failed to read Solana keypair from file: {}", e))?
        } else {
            Keypair::new()
        };

        Ok(SolanaSurfnetFacilitatorConfig {
            rpc_url,
            payer_keypair,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaMainnetFacilitatorProviderConfigFile {
    pub payer_keypair_path: Option<String>,
    pub rpc_url: Option<String>,
}

impl TryInto<SolanaMainnetFacilitatorConfig> for &SolanaMainnetFacilitatorProviderConfigFile {
    type Error = String;
    fn try_into(self) -> Result<SolanaMainnetFacilitatorConfig, Self::Error> {
        let rpc_url = if let Some(ref url) = self.rpc_url {
            url.parse::<Url>()
                .map_err(|e| format!("Failed to parse RPC URL {}: {}", url, e))?
        } else {
            "https://api.mainnet-beta.solana.com"
                .parse::<Url>()
                .expect("Failed to parse default Mainnet RPC URL")
        };

        let payer_keypair = if let Some(ref path) = self.payer_keypair_path {
            Keypair::read_from_file(path)
                .map_err(|e| format!("Failed to read Solana keypair from file: {}", e))?
        } else {
            Keypair::new()
        };

        Ok(SolanaMainnetFacilitatorConfig {
            rpc_url,
            payer_keypair,
        })
    }
}
/// X402 sandbox/test configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X402SandboxConfig {
    /// Optional description of this sandbox environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Configuration of the networks supported by this provider
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub billing_networks: IndexMap<String, BillingNetworkConfigFile>,

    /// Facilitator config
    pub facilitator: FacilitatorConfigFile,
}
