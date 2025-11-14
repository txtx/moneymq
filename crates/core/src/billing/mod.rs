use indexmap::IndexMap;
use moneymq_types::x402::{
    MoneyMqNetwork, Network,
    config::facilitator::{ValidatorNetworkConfig, ValidatorsConfig},
};
use solana_client::rpc_client::RpcClient;
use tracing::{debug, info};
use url::Url;

use crate::{
    billing::{currency::Currency, recipient::Recipient},
    validator::surfnet_utils::{
        SetAccountRequest, SetTokenAccountRequest, surfnet_set_account, surfnet_set_token_account,
    },
};

pub mod currency;
pub mod recipient;

/// Manages billing configurations across multiple networks
#[derive(Debug, Clone)]
pub struct NetworksConfig {
    /// Mapping of network names to their billing configurations
    pub configs: IndexMap<String, NetworkConfig>,
    /// Mapping of networks to their names
    pub lookup: IndexMap<Network, String>,
}

impl NetworksConfig {
    /// Initializes the [NetworksConfig] with the provided network configurations
    pub fn initialize(
        networks_map: IndexMap<
            String,
            (MoneyMqNetwork, Option<String>, Vec<String>), // (network, payment_recipient, currencies)
        >,
        is_sandbox: bool,
    ) -> Result<Self, NetworksConfigError> {
        let mut configs = IndexMap::new();
        let mut lookup = IndexMap::new();

        for (network_name, (moneymq_network, payment_recipient_opt, currencies_strs)) in
            networks_map.into_iter()
        {
            let network: Network = moneymq_network.clone().into();
            let mut currencies = vec![];
            for symbol in currencies_strs {
                let currency =
                    Currency::from_symbol_and_network(&symbol, &network).map_err(|e| {
                        NetworksConfigError::InitializationError(network.clone(), e.to_string())
                    })?;
                currencies.push(currency);
            }

            let payment_recipient = Recipient::instantiate_payment_recipient(
                &network,
                payment_recipient_opt.as_ref(),
                is_sandbox,
            )
            .map_err(|e| {
                NetworksConfigError::InitializationError(network.clone(), e.to_string())
            })?;

            let network_config = match moneymq_network {
                MoneyMqNetwork::SolanaSurfnet => {
                    let cap = 10;
                    let mut user_accounts = Vec::with_capacity(cap);
                    info!("Initializing {} pre-funded accounts", user_accounts.len());
                    for i in 0..cap {
                        let recipient =
                            Recipient::instantiate_with_index(&network, None, is_sandbox, Some(i))
                                .map_err(|e| {
                                    NetworksConfigError::InitializationError(
                                        network.clone(),
                                        e.to_string(),
                                    )
                                })?;
                        debug!("User account {}: {:?}", i, recipient);

                        user_accounts.push(recipient);
                    }

                    NetworkConfig::SolanaSurfnet(SolanaSurfnetConfig {
                        payment_recipient,
                        currencies,
                        user_accounts,
                    })
                }
                MoneyMqNetwork::SolanaMainnet => {
                    NetworkConfig::SolanaMainnet(SolanaMainnetConfig {
                        payment_recipient,
                        currencies,
                    })
                }
            };

            lookup.insert(network, network_name.clone());

            configs.insert(network_name, network_config);
        }

        Ok(NetworksConfig { configs, lookup })
    }

    /// Funds local accounts and MoneyMQ-managed accounts
    pub async fn fund_accounts(&self, validators_config: &ValidatorsConfig) -> Result<(), String> {
        for (_, network_config) in self.configs.iter() {
            let recipient = network_config.recipient();
            let address = recipient.address();
            let pubkey = address.pubkey().expect("Expected Solana address");

            match network_config {
                NetworkConfig::SolanaSurfnet(surfnet_cfg) => {
                    let Some(ValidatorNetworkConfig::SolanaSurfnet(surfnet_rpc_config)) =
                        validators_config
                            .networks
                            .get(&Network::Solana.to_string())
                            .cloned()
                    else {
                        continue;
                    };
                    let rpc_url = surfnet_rpc_config.rpc_url;
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
                        let is_pay_to = address.eq(pubkey);
                        let token_amount = if is_pay_to { 0 } else { 500_000_000_000 };

                        surfnet_set_account(
                            &rpc_client,
                            SetAccountRequest::new(*address).lamports(1_000_000_000),
                        )?;
                        for currency in network_config.currencies() {
                            #[allow(irrefutable_let_patterns)]
                            if let Currency::Solana(solana_currency) = currency {
                                debug!(
                                    "Setting up token account for address {} with mint {}",
                                    address, solana_currency.mint
                                );
                                surfnet_set_token_account(
                                    &rpc_client,
                                    SetTokenAccountRequest::new(
                                        *address,
                                        solana_currency.mint,
                                        solana_currency.token_program,
                                    )
                                    .amount(token_amount),
                                )?;
                            }
                        }
                    }
                }
                NetworkConfig::SolanaMainnet(_) => {
                    if recipient.is_managed() {
                        // TODO: Fund mainnet accounts if MoneyMqManaged
                    }
                }
            }
        }

        Ok(())
    }

    /// Retrieves the billing configuration for the specified network
    pub fn get_config_for_network(&self, network: &Network) -> Option<&NetworkConfig> {
        self.lookup
            .get(network)
            .and_then(|name| self.configs.get(name))
    }

    pub fn get_network_for_name(&self, name: &str) -> Option<MoneyMqNetwork> {
        self.configs.get(name).map(|config| config.network())
    }
}

#[derive(Debug, Clone)]
pub enum NetworkConfig {
    SolanaSurfnet(SolanaSurfnetConfig),
    SolanaMainnet(SolanaMainnetConfig),
}

impl NetworkConfig {
    pub fn recipient(&self) -> &Recipient {
        match self {
            NetworkConfig::SolanaSurfnet(cfg) => &cfg.payment_recipient,
            NetworkConfig::SolanaMainnet(cfg) => &cfg.payment_recipient,
        }
    }
    pub fn currencies(&self) -> &Vec<Currency> {
        match self {
            NetworkConfig::SolanaSurfnet(cfg) => &cfg.currencies,
            NetworkConfig::SolanaMainnet(cfg) => &cfg.currencies,
        }
    }
    pub fn default_rpc_url(&self) -> Url {
        match self {
            NetworkConfig::SolanaSurfnet(_) => "http://localhost:8899".parse().unwrap(),
            NetworkConfig::SolanaMainnet(_) => {
                "https://api.mainnet-beta.solana.com".parse().unwrap()
            }
        }
    }
    pub fn surfnet_config(&self) -> Option<&SolanaSurfnetConfig> {
        match self {
            NetworkConfig::SolanaSurfnet(cfg) => Some(cfg),
            _ => None,
        }
    }
    pub fn network(&self) -> MoneyMqNetwork {
        match self {
            NetworkConfig::SolanaSurfnet(_) => MoneyMqNetwork::SolanaSurfnet,
            NetworkConfig::SolanaMainnet(_) => MoneyMqNetwork::SolanaMainnet,
        }
    }
    pub fn user_accounts(&self) -> Vec<Recipient> {
        match self {
            NetworkConfig::SolanaSurfnet(cfg) => cfg.user_accounts.clone(),
            NetworkConfig::SolanaMainnet(_) => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolanaSurfnetConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
    pub user_accounts: Vec<Recipient>,
}

#[derive(Debug, Clone)]
pub struct SolanaMainnetConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
}

#[derive(Debug, thiserror::Error)]
pub enum NetworksConfigError {
    #[error("Failed to initialize network {0}: {1}")]
    InitializationError(Network, String),
}
