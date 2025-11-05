use indexmap::IndexMap;
use moneymq_types::x402::{MoneyMqNetwork, Network};
use solana_client::rpc_client::RpcClient;
use tracing::{debug, info};
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
            (MoneyMqNetwork, Option<String>, Vec<String>, Vec<String>), // (network, payment_recipient, currencies, user_accounts)
        >,
        is_sandbox: bool,
    ) -> Result<Self, BillingManagerError> {
        let mut configs = IndexMap::new();
        let mut network_name_map = IndexMap::new();

        for (
            network_name,
            (moneymq_network, payment_recipient_opt, currencies_strs, user_accounts_strs),
        ) in networks_map.into_iter()
        {
            let network: Network = moneymq_network.clone().into();
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
                Recipient::instantiate(&network, payment_recipient_opt.as_ref(), is_sandbox)
                    .await
                    .map_err(|e| {
                        BillingManagerError::InitializationError(network.clone(), e.to_string())
                    })?;

            let billing_config = match moneymq_network {
                MoneyMqNetwork::SolanaSurfnet => {
                    let cap = 10.max(user_accounts_strs.len());
                    let mut user_accounts = Vec::with_capacity(cap);
                    debug!("Initializing {} user accounts", user_accounts_strs.len());
                    for i in 0..cap {
                        let some_provided_account = user_accounts_strs.get(i);
                        let recipient =
                            Recipient::instantiate(&network, some_provided_account, is_sandbox)
                                .await
                                .map_err(|e| {
                                    BillingManagerError::InitializationError(
                                        network.clone(),
                                        e.to_string(),
                                    )
                                })?;
                        debug!("User account {}: {:?}", i, some_provided_account);

                        user_accounts.push(recipient);
                    }

                    NetworkBillingConfig::SolanaSurfnet(SolanaSurfnetBillingConfig {
                        payment_recipient,
                        currencies,
                        user_accounts,
                    })
                }
                MoneyMqNetwork::SolanaMainnet => {
                    NetworkBillingConfig::SolanaMainnet(SolanaMainnetBillingConfig {
                        payment_recipient,
                        currencies,
                    })
                }
            };

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
                NetworkBillingConfig::SolanaSurfnet(surfnet_cfg) => {
                    let rpc_url = local_validator_rpc_urls
                        .get(&Network::Solana)
                        .cloned()
                        .unwrap_or(default_rpc_url);
                    let rpc_client = RpcClient::new(rpc_url.as_str());

                    info!(
                        "Initializing local Solana x402 Recipient account {} on network {}",
                        pubkey, rpc_url
                    );
                    info!(
                        "Initializing {} user accounts",
                        surfnet_cfg.user_accounts.len()
                    );

                    let user_addresses = surfnet_cfg
                        .user_accounts
                        .iter()
                        .map(|recipient| {
                            let user_address = recipient.address();
                            user_address
                                .pubkey()
                                .expect("Expected Solana address")
                                .clone()
                        })
                        .collect::<Vec<_>>();

                    let all_addresses = std::iter::once(pubkey).chain(user_addresses.iter());

                    for address in all_addresses {
                        for currency in billing_config.currencies() {
                            #[allow(irrefutable_let_patterns)]
                            if let Currency::Solana(solana_currency) = currency {
                                debug!(
                                    "Setting up token account for address {} with mint {}",
                                    address, solana_currency.mint
                                );
                                surfnet_set_token_account(
                                    &rpc_client,
                                    address,
                                    &solana_currency.mint,
                                    &solana_currency.token_program,
                                )?;
                                surfnet_set_account(&rpc_client, &address)?;
                            }
                        }
                    }
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

    pub fn get_network_for_name(&self, name: &str) -> Option<MoneyMqNetwork> {
        self.configs.get(name).map(|config| config.network())
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
    pub fn network(&self) -> MoneyMqNetwork {
        match self {
            NetworkBillingConfig::SolanaSurfnet(_) => MoneyMqNetwork::SolanaSurfnet,
            NetworkBillingConfig::SolanaMainnet(_) => MoneyMqNetwork::SolanaMainnet,
        }
    }
    pub fn user_accounts(&self) -> Vec<Recipient> {
        match self {
            NetworkBillingConfig::SolanaSurfnet(cfg) => cfg.user_accounts.clone(),
            NetworkBillingConfig::SolanaMainnet(_) => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolanaSurfnetBillingConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
    pub user_accounts: Vec<Recipient>,
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
