use std::collections::HashMap;

use indexmap::IndexMap;
use moneymq_types::x402::config::{
    constants::{
        DEFAULT_BINDING_ADDRESS, DEFAULT_FACILITATOR_PORT, DEFAULT_RPC_PORT, DEFAULT_SANDBOX,
        DEFAULT_WS_PORT,
    },
    facilitator::{
        FacilitatorConfig as FacilitatorRuntimeConfig, FacilitatorNetworkConfig,
        FacilitatorRpcConfig, SolanaSurfnetFacilitatorConfig,
    },
};
use serde::{Deserialize, Serialize};
use solana_keypair::{EncodableKey, Keypair};
use url::Url;

/// Payment configuration with protocol as enum tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "protocol")]
pub enum PaymentConfig {
    #[serde(rename = "x402")]
    X402(X402PaymentConfig),
}

impl Default for PaymentConfig {
    fn default() -> Self {
        PaymentConfig::X402(X402PaymentConfig::default())
    }
}

/// Facilitator configuration - either a service URL or local config
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FacilitatorConfig {
    /// Remote facilitator service URL
    ServiceUrl { service_url: String },
    /// Local facilitator configuration
    Embedded(SandboxFacilitatorConfig),
}

impl Default for FacilitatorConfig {
    fn default() -> Self {
        FacilitatorConfig::Embedded(SandboxFacilitatorConfig::default())
    }
}

/// X402 payment protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X402PaymentConfig {
    /// Optional description of this payment network
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Facilitator configuration for production
    #[serde(default)]
    pub facilitator: FacilitatorConfig,

    /// Accepted networks for production payments
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub accepted: IndexMap<NetworkIdentifier, AcceptedNetworkConfig>,

    /// Nested sandbox/test configurations
    /// Key is the sandbox name (e.g., "default", "staging", "test")
    /// When --sandbox flag is used, the "default" sandbox will be used
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub sandboxes: IndexMap<String, X402SandboxConfig>,
}

impl X402PaymentConfig {
    /// Get the default sandbox configuration
    pub fn get_default_sandbox(&self) -> Option<&X402SandboxConfig> {
        self.sandboxes.get(DEFAULT_SANDBOX)
    }
}

/// Accepted network configuration for production
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedNetworkConfig {
    /// Payment recipient address (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// Accepted currencies for this network
    pub currencies: Vec<String>,
}

/// Network identifier for configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkIdentifier {
    Solana,
}

impl std::fmt::Display for NetworkIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkIdentifier::Solana => write!(f, "solana"),
        }
    }
}

/// X402 sandbox/test configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct X402SandboxConfig {
    /// Optional description of this sandbox environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Facilitator configuration for sandbox
    #[serde(default)]
    pub facilitator: FacilitatorConfig,

    /// Validator configuration for sandbox
    #[serde(default)]
    pub validator: ValidatorConfig,
}

/// Sandbox facilitator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxFacilitatorConfig {
    /// Binding address for the facilitator server
    #[serde(default)]
    pub binding_address: String,

    /// Binding port for the facilitator server
    #[serde(default)]
    pub binding_port: u16,

    /// Supported networks for sandbox facilitator
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub supported: IndexMap<NetworkIdentifier, SupportedNetworkConfig>,
}

impl Default for SandboxFacilitatorConfig {
    fn default() -> Self {
        Self {
            binding_address: DEFAULT_BINDING_ADDRESS.to_string(),
            binding_port: DEFAULT_FACILITATOR_PORT,
            supported: IndexMap::new(),
        }
    }
}

/// Supported network configuration for sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedNetworkConfig {
    /// Payment recipient address (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,

    /// Supported currencies for this network
    pub currencies: Vec<String>,

    /// Fee amount (in smallest unit)
    #[serde(default)]
    pub fee: u64,

    /// Optional payer keypair path for this network
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer_keypair_path: Option<String>,

    /// Optional RPC URL for this network
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,

    /// Optional user accounts to fund in sandbox
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_accounts: Vec<String>,
}

/// Validator configuration for sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorConfig {
    /// Binding address for the validator
    #[serde(default)]
    pub binding_address: String,

    /// RPC binding port for the validator
    #[serde(default)]
    pub rpc_binding_port: u16,

    /// WebSocket binding port for the validator
    #[serde(default)]
    pub ws_binding_port: u16,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            binding_address: DEFAULT_BINDING_ADDRESS.to_string(),
            rpc_binding_port: DEFAULT_RPC_PORT,
            ws_binding_port: DEFAULT_WS_PORT,
        }
    }
}

// Conversion implementations to maintain compatibility with existing code

impl TryInto<FacilitatorRuntimeConfig> for &X402SandboxConfig {
    type Error = String;
    fn try_into(self) -> Result<FacilitatorRuntimeConfig, Self::Error> {
        let facilitator_config = match &self.facilitator {
            FacilitatorConfig::Embedded(config) => config,
            FacilitatorConfig::ServiceUrl { service_url } => {
                return Err(format!(
                    "Cannot convert sandbox with remote facilitator URL '{}' to FacilitatorRuntimeConfig",
                    service_url
                ));
            }
        };

        let mut networks = HashMap::new();

        // Convert supported networks to facilitator network configs
        for (network_id, network_config) in &facilitator_config.supported {
            let rpc_config = if let Some(ref url) = network_config.rpc_url {
                FacilitatorRpcConfig::from_url(url)?.with_ws_port(self.validator.ws_binding_port)
            } else {
                FacilitatorRpcConfig::from_parts(
                    &self.validator.binding_address,
                    self.validator.rpc_binding_port,
                    self.validator.ws_binding_port,
                )?
            };

            let payer_keypair = if let Some(ref path) = network_config.payer_keypair_path {
                Keypair::read_from_file(path)
                    .map_err(|e| format!("Failed to read Solana keypair from file: {}", e))?
            } else {
                Keypair::new()
            };

            // For now, all networks are treated as SolanaSurfnet in sandbox
            networks.insert(
                network_id.to_string(),
                FacilitatorNetworkConfig::SolanaSurfnet(SolanaSurfnetFacilitatorConfig {
                    rpc_config,
                    payer_keypair,
                }),
            );
        }

        let url = format!(
            "http://{}:{}",
            facilitator_config.binding_address, facilitator_config.binding_port
        )
        .parse::<Url>()
        .map_err(|e| format!("Failed to parse facilitator URL: {}", e))?;

        Ok(FacilitatorRuntimeConfig { url, networks })
    }
}
