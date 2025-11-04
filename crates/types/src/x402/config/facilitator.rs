use std::collections::HashMap;

use solana_keypair::{Keypair, Signer};
use url::Url;

use crate::x402::{Network, SupportedPaymentKindExtra};

#[derive(Debug)]
pub struct FacilitatorConfig {
    pub url: Url,
    pub api_token: Option<String>,
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
            FacilitatorNetworkConfig::SolanaSurfnet(_) => Network::SolanaSurfnet,
            FacilitatorNetworkConfig::SolanaMainnet(_) => Network::SolanaMainnet,
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
