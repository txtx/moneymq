use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use solana_keypair::{Keypair, Signer};
use url::Url;

use crate::x402::{Network, SupportedPaymentKindExtra};

#[derive(Debug)]
pub struct FacilitatorConfig {
    pub url: Url,
    pub networks: HashMap<String, FacilitatorNetworkConfig>,
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
            FacilitatorNetworkConfig::SolanaSurfnet(cfg) => &cfg.rpc_url,
            FacilitatorNetworkConfig::SolanaMainnet(cfg) => &cfg.rpc_url,
        }
    }
}

#[derive(Debug)]
pub struct SolanaSurfnetFacilitatorConfig {
    pub rpc_url: Url,
    pub payer_keypair: Keypair,
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
    pub rpc_url: Url,
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
            self.rpc_url,
            self.payer_keypair.pubkey()
        )
    }
}

impl Display for SolanaMainnetFacilitatorConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SolanaMainnet {{ rpc_url: {}, payer_pubkey: {} }}",
            self.rpc_url,
            self.payer_keypair.pubkey()
        )
    }
}
