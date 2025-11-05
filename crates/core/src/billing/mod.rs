use indexmap::IndexMap;
use moneymq_types::x402::Network;
use solana_client::rpc_client::RpcClient;
use tracing::info;
use url::Url;

use crate::{
    billing::{currency::Currency, recipient::Recipient},
    validator::surfnet_utils::{surfnet_set_account, surfnet_set_token_account},
};

pub mod currency;
pub mod recipient;

/// Manages billing configurations across multiple networks
#[derive(Debug, Clone)]
pub struct BillingManager {
    /// Mapping of network names to their billing configurations
    pub configs: IndexMap<String, NetworkBillingConfig>,
    /// Mapping of networks to their names
    pub network_name_map: IndexMap<Network, String>,
}

impl BillingManager {
    /// Initializes the [BillingManager] with the provided network configurations
    pub async fn initialize(
        networks_map: IndexMap<
            String,
            (Network, Option<String>, Vec<String>), // (network, payment_recipient, currencies)
        >,
    ) -> Result<Self, BillingManagerError> {
        let mut configs = IndexMap::new();
        let mut network_name_map = IndexMap::new();

        for (network_name, (network, payment_recipient_opt, currencies_strs)) in
            networks_map.into_iter()
        {
            let mut currencies = vec![];
            for symbol in currencies_strs {
                let currency = Currency::from_symbol_and_network(&symbol, &network)
                    .await
                    .map_err(|e| {
                        BillingManagerError::InitializationError(network.clone(), e.to_string())
                    })?;
                currencies.push(currency);
            }

            let payment_recipient =
                Recipient::instantiate(&network, payment_recipient_opt.as_ref())
                    .await
                    .map_err(|e| {
                        BillingManagerError::InitializationError(network.clone(), e.to_string())
                    })?;

            let billing_config = NetworkBillingConfig::SolanaSurfnet(SolanaSurfnetBillingConfig {
                payment_recipient,
                currencies,
            });

            network_name_map.insert(network, network_name.clone());

            configs.insert(network_name, billing_config);
        }

        Ok(BillingManager {
            configs,
            network_name_map,
        })
    }

    /// Funds local accounts and MoneyMQ-managed accounts
    pub async fn fund_accounts(
        &self,
        local_validator_rpc_urls: &IndexMap<Network, url::Url>,
    ) -> Result<(), String> {
        for (_, billing_config) in self.configs.iter() {
            let recipient = billing_config.recipient();
            let address = recipient.address();
            let pubkey = address.pubkey().expect("Expected Solana address");
            let default_rpc_url = billing_config.default_rpc_url();

            match billing_config {
                NetworkBillingConfig::SolanaSurfnet(_) => {
                    let rpc_url = local_validator_rpc_urls
                        .get(&Network::Solana)
                        .cloned()
                        .unwrap_or(default_rpc_url);
                    let rpc_client = RpcClient::new(rpc_url.as_str());
                    surfnet_set_account(&rpc_client, pubkey)?;
                    for currency in billing_config.currencies() {
                        #[allow(irrefutable_let_patterns)]
                        if let Currency::Solana(solana_currency) = currency {
                            surfnet_set_token_account(
                                &rpc_client,
                                pubkey,
                                &solana_currency.mint,
                                &solana_currency.token_program,
                            )?;
                        }
                    }

                    info!(
                        "Initializing local Solana account {} on network {}",
                        pubkey, rpc_url
                    );
                }
                NetworkBillingConfig::SolanaMainnet(_) => {
                    if recipient.is_managed() {
                        // TODO: Fund mainnet accounts if MoneyMqManaged
                    }
                }
            }
        }

        Ok(())
    }

    /// Retrieves the billing configuration for the specified network
    pub fn get_config_for_network(&self, network: &Network) -> Option<&NetworkBillingConfig> {
        self.network_name_map
            .get(network)
            .and_then(|name| self.configs.get(name))
    }
}

#[derive(Debug, Clone)]
pub enum NetworkBillingConfig {
    SolanaSurfnet(SolanaSurfnetBillingConfig),
    SolanaMainnet(SolanaMainnetBillingConfig),
}

impl NetworkBillingConfig {
    pub fn recipient(&self) -> &Recipient {
        match self {
            NetworkBillingConfig::SolanaSurfnet(cfg) => &cfg.payment_recipient,
            NetworkBillingConfig::SolanaMainnet(cfg) => &cfg.payment_recipient,
        }
    }
    pub fn currencies(&self) -> &Vec<Currency> {
        match self {
            NetworkBillingConfig::SolanaSurfnet(cfg) => &cfg.currencies,
            NetworkBillingConfig::SolanaMainnet(cfg) => &cfg.currencies,
        }
    }
    pub fn default_rpc_url(&self) -> Url {
        match self {
            NetworkBillingConfig::SolanaSurfnet(_) => "http://localhost:8899".parse().unwrap(),
            NetworkBillingConfig::SolanaMainnet(_) => {
                "https://api.mainnet-beta.solana.com".parse().unwrap()
            }
        }
    }
    pub fn surfnet_config(&self) -> Option<&SolanaSurfnetBillingConfig> {
        match self {
            NetworkBillingConfig::SolanaSurfnet(cfg) => Some(cfg),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolanaSurfnetBillingConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
}

#[derive(Debug, Clone)]
pub struct SolanaMainnetBillingConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
}

#[derive(Debug, thiserror::Error)]
pub enum BillingManagerError {
    #[error("Failed to initialize network {0}: {1}")]
    InitializationError(Network, String),
}
