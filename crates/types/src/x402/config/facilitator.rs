use solana_keypair::{Keypair, Signer};
use std::{
    collections::HashMap,
    fmt::{self, Display},
};
use url::Url;

use crate::x402::{
    Network, SupportedPaymentKindExtra,
    config::constants::{DEFAULT_BINDING_ADDRESS, DEFAULT_RPC_PORT},
};

#[derive(Debug)]
pub struct FacilitatorConfig {
    pub url: Url,
    pub networks: HashMap<String, FacilitatorNetworkConfig>,
}

impl FacilitatorConfig {
    pub fn get_facilitator_pubkey(&self, name: &str) -> Option<String> {
        self.networks.get(name).and_then(|config| match config {
            FacilitatorNetworkConfig::SolanaSurfnet(cfg) => {
                Some(cfg.payer_keypair.pubkey().to_string())
            }
            FacilitatorNetworkConfig::SolanaMainnet(cfg) => {
                Some(cfg.payer_keypair.pubkey().to_string())
            }
        })
    }
}

#[derive(Debug)]
pub enum FacilitatorNetworkConfig {
    SolanaSurfnet(SolanaSurfnetFacilitatorConfig),
    SolanaMainnet(SolanaMainnetFacilitatorConfig),
}

impl FacilitatorNetworkConfig {
    pub fn network(&self) -> Network {
        match self {
            FacilitatorNetworkConfig::SolanaSurfnet(_) => Network::Solana,
            FacilitatorNetworkConfig::SolanaMainnet(_) => Network::Solana,
        }
    }
    pub fn extra(&self) -> Option<SupportedPaymentKindExtra> {
        match self {
            FacilitatorNetworkConfig::SolanaSurfnet(cfg) => cfg.extra(),
            FacilitatorNetworkConfig::SolanaMainnet(cfg) => cfg.extra(),
        }
    }
    pub fn rpc_url(&self) -> &Url {
        match self {
            FacilitatorNetworkConfig::SolanaSurfnet(cfg) => &cfg.rpc_config.rpc_url,
            FacilitatorNetworkConfig::SolanaMainnet(cfg) => &cfg.rpc_config.rpc_url,
        }
    }
}

#[derive(Debug)]
pub struct SolanaSurfnetFacilitatorConfig {
    pub rpc_config: FacilitatorRpcConfig,
    pub payer_keypair: Keypair,
}

impl Default for SolanaSurfnetFacilitatorConfig {
    fn default() -> Self {
        Self {
            rpc_config: FacilitatorRpcConfig {
                rpc_url: format!("http://{}:{}", DEFAULT_BINDING_ADDRESS, DEFAULT_RPC_PORT)
                    .parse::<Url>()
                    .expect("Failed to parse default RPC URL"),
                bind_host: Some("0.0.0.0".to_string()),
                rpc_port: Some(8899),
                ws_port: Some(8900),
            },
            payer_keypair: Keypair::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FacilitatorRpcConfig {
    pub rpc_url: Url,
    pub bind_host: Option<String>,
    pub rpc_port: Option<u16>,
    pub ws_port: Option<u16>,
}
impl FacilitatorRpcConfig {
    pub fn from_url(url: &str) -> Result<Self, String> {
        let rpc_url = url
            .parse::<Url>()
            .map_err(|e| format!("Failed to parse RPC URL {}: {}", url, e))?;
        Ok(Self {
            rpc_url,
            bind_host: None,
            rpc_port: None,
            ws_port: None,
        })
    }
    pub fn with_ws_port(mut self, ws_port: u16) -> Self {
        self.ws_port = Some(ws_port);
        self
    }
    pub fn from_parts(bind_host: &str, rpc_port: u16, ws_port: u16) -> Result<Self, String> {
        let rpc_url = format!("http://{}:{}", bind_host, rpc_port)
            .parse::<Url>()
            .map_err(|e| format!("Failed to parse validator RPC URL: {}", e))?;
        Ok(Self {
            rpc_url,
            bind_host: Some(bind_host.to_string()),
            rpc_port: Some(rpc_port),
            ws_port: Some(ws_port),
        })
    }
}

impl SolanaSurfnetFacilitatorConfig {
    pub fn extra(&self) -> Option<SupportedPaymentKindExtra> {
        Some(SupportedPaymentKindExtra {
            fee_payer: self.payer_keypair.pubkey().into(),
        })
    }
}

#[derive(Debug)]
pub struct SolanaMainnetFacilitatorConfig {
    pub rpc_config: FacilitatorRpcConfig,
    pub payer_keypair: Keypair,
}

impl SolanaMainnetFacilitatorConfig {
    pub fn extra(&self) -> Option<SupportedPaymentKindExtra> {
        Some(SupportedPaymentKindExtra {
            fee_payer: self.payer_keypair.pubkey().into(),
        })
    }
}

impl Display for FacilitatorConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FacilitatorConfig {{")?;
        writeln!(f, "  url: {}", self.url)?;
        writeln!(f, "  networks: {{")?;
        for (name, config) in &self.networks {
            writeln!(f, "    {}: {}", name, config)?;
        }
        writeln!(f, "  }}")?;
        write!(f, "}}")
    }
}

impl Display for FacilitatorNetworkConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FacilitatorNetworkConfig::SolanaSurfnet(config) => write!(f, "{}", config),
            FacilitatorNetworkConfig::SolanaMainnet(config) => write!(f, "{}", config),
        }
    }
}

impl Display for SolanaSurfnetFacilitatorConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SolanaSurfnet {{ rpc_url: {}, payer_pubkey: {} }}",
            self.rpc_config.rpc_url,
            self.payer_keypair.pubkey()
        )
    }
}

impl Display for SolanaMainnetFacilitatorConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SolanaMainnet {{ rpc_url: {}, payer_pubkey: {} }}",
            self.rpc_config.rpc_url,
            self.payer_keypair.pubkey()
        )
    }
}
