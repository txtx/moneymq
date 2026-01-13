//! Sandbox module for local development and testing
//!
//! This module provides:
//! - Network configuration management for sandbox environments
//! - Pre-funded user accounts for testing
//! - Sandbox-specific API endpoints

use indexmap::IndexMap;
use moneymq_types::{
    AccountConfig, AccountRole, AccountsConfig, Base58Keychain, Keychain, OperatorRole,
    x402::{
        Currency, LocalManagedRecipient, MoneyMqManagedRecipient, MoneyMqNetwork, Network,
        Recipient,
        config::facilitator::{ValidatorNetworkConfig, ValidatorsConfig},
    },
};
use sha2::{Digest, Sha256};
use solana_client::rpc_client::RpcClient;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use tracing::{debug, info};
use url::Url;

/// Seed phrase for generating the sandbox facilitator keypair
pub const SANDBOX_FACILITATOR_SEED: &str = "moneymq-sandbox-payment-api-fee-payer-v1";

use crate::validator::surfnet_utils::{SetTokenAccountRequest, surfnet_set_token_account};

mod accounts;
pub use accounts::list_accounts;

/// Generate sandbox operator accounts from the user accounts in networks config
///
/// Creates:
/// - 1 facilitator account using a deterministic keypair from SANDBOX_FACILITATOR_SEED
/// - Up to 5 operator accounts from the first 5 user accounts in the networks config
///
/// Each operator account uses the user account's keypair for signing transactions.
pub fn generate_sandbox_accounts(networks_config: &NetworksConfig) -> AccountsConfig {
    let mut accounts = IndexMap::new();

    // Generate the facilitator account using deterministic seed
    let mut hasher = Sha256::new();
    hasher.update(SANDBOX_FACILITATOR_SEED.as_bytes());
    let seed = hasher.finalize();
    let seed_array: [u8; 32] = seed[..32].try_into().unwrap();
    let facilitator_keypair = Keypair::new_from_array(seed_array);
    let facilitator_secret = facilitator_keypair.to_base58_string();

    let facilitator_account = AccountConfig {
        id: "facilitator".to_string(),
        name: "Facilitator (fee payer)".to_string(),
        role: AccountRole::Operator(OperatorRole {
            keychain: Keychain::Base58(Base58Keychain {
                secret: facilitator_secret,
            }),
        }),
        currency_mapping: IndexMap::new(),
    };
    accounts.insert("facilitator".to_string(), facilitator_account);

    // Get user accounts from the first network config (typically solana surfnet)
    for (_, config) in &networks_config.configs {
        let user_accounts = config.user_accounts();

        // Create operator accounts from the first 5 user accounts
        for (i, recipient) in user_accounts.iter().take(5).enumerate() {
            if let Recipient::MoneyMqManaged(MoneyMqManagedRecipient::Local(
                LocalManagedRecipient {
                    address: _,
                    keypair_bytes,
                    label,
                },
            )) = recipient
            {
                let id = label.clone().unwrap_or_else(|| format!("operator_{}", i));
                let name = label
                    .as_ref()
                    .map(|l| format!("{} (operator)", l))
                    .unwrap_or_else(|| format!("Operator {}", i));

                // Encode the keypair bytes as base58
                let secret = bs58::encode(keypair_bytes).into_string();

                let account = AccountConfig {
                    id: id.clone(),
                    name,
                    role: AccountRole::Operator(OperatorRole {
                        keychain: Keychain::Base58(Base58Keychain { secret }),
                    }),
                    currency_mapping: IndexMap::new(),
                };

                accounts.insert(id, account);
            }
        }

        // Only process the first network config
        break;
    }

    accounts
}

/// Initial USDC token amount for user accounts in local surfnet (2000 USDC with 6 decimals)
pub const INITIAL_USER_USDC_AMOUNT: u64 = 2_000_000_000;

/// Token configuration for funding sandbox accounts
#[derive(Debug, Clone)]
pub struct SandboxTokenConfig {
    pub mint: Pubkey,
    pub token_program: Pubkey,
}

/// Fund a single sandbox account with tokens
///
/// Creates/updates token accounts for each token config with the specified amount.
/// Note: SOL is not funded as sandbox accounts use fee payers for gas.
///
/// # Arguments
/// * `rpc_client` - RPC client connected to the Surfnet validator
/// * `address` - The account address to fund
/// * `tokens` - Token configurations (mint + program)
/// * `token_amount` - Amount of tokens to fund (use 0 for payment recipient accounts)
pub fn fund_sandbox_account(
    rpc_client: &RpcClient,
    address: Pubkey,
    tokens: &[SandboxTokenConfig],
    token_amount: u64,
) -> Result<(), String> {
    for token in tokens {
        debug!(
            "Setting up token account for address {} with mint {}",
            address, token.mint
        );
        surfnet_set_token_account(
            rpc_client,
            SetTokenAccountRequest::new(address, token.mint, token.token_program)
                .amount(token_amount),
        )?;
    }

    Ok(())
}

/// Fund multiple sandbox accounts with tokens
///
/// This is a convenience function that calls `fund_sandbox_account` for each address.
/// All accounts receive the same token amount (use `INITIAL_USER_USDC_AMOUNT` for user accounts).
///
/// # Arguments
/// * `rpc_url` - URL of the Surfnet validator RPC
/// * `addresses` - List of account addresses to fund
/// * `tokens` - Token configurations (mint + program)
/// * `token_amount` - Amount of tokens to fund each account
pub fn fund_sandbox_accounts(
    rpc_url: &str,
    addresses: &[Pubkey],
    tokens: &[SandboxTokenConfig],
    token_amount: u64,
) -> Result<(), String> {
    let rpc_client = RpcClient::new(rpc_url);

    info!(
        "Funding {} sandbox accounts with {} tokens each",
        addresses.len(),
        token_amount
    );

    for address in addresses {
        fund_sandbox_account(&rpc_client, *address, tokens, token_amount)?;
    }

    Ok(())
}

/// Manages network configurations across multiple networks
#[derive(Debug, Clone)]
pub struct NetworksConfig {
    /// Mapping of network names to their configurations
    pub configs: IndexMap<String, NetworkConfig>,
    /// Mapping of networks to their names
    pub lookup: IndexMap<Network, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum NetworksConfigError {
    #[error("Failed to initialize network {0}: {1}")]
    InitializationError(Network, String),
}

impl Default for NetworksConfig {
    fn default() -> Self {
        Self {
            configs: IndexMap::new(),
            lookup: IndexMap::new(),
        }
    }
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

                    // Build token configs from currencies
                    let tokens: Vec<SandboxTokenConfig> = network_config
                        .currencies()
                        .iter()
                        .map(|currency| {
                            let Currency::Solana(solana_currency) = currency;
                            SandboxTokenConfig {
                                mint: solana_currency.mint,
                                token_program: solana_currency.token_program,
                            }
                        })
                        .collect();

                    // Fund payment recipient account (with 0 tokens - it just receives)
                    fund_sandbox_account(&rpc_client, *pubkey, &tokens, 0)?;

                    // Fund user accounts with INITIAL_USER_USDC_AMOUNT
                    let user_addresses: Vec<Pubkey> = surfnet_cfg
                        .user_accounts
                        .iter()
                        .map(|recipient| {
                            *recipient
                                .address()
                                .pubkey()
                                .expect("Expected Solana address")
                        })
                        .collect();

                    for address in &user_addresses {
                        fund_sandbox_account(
                            &rpc_client,
                            *address,
                            &tokens,
                            INITIAL_USER_USDC_AMOUNT,
                        )?;
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

    /// Retrieves the configuration for the specified network
    pub fn get_config_for_network(&self, network: &Network) -> Option<&NetworkConfig> {
        self.lookup
            .get(network)
            .and_then(|name| self.configs.get(name))
    }

    /// Gets the MoneyMQ network type for a given network name
    pub fn get_network_for_name(&self, name: &str) -> Option<MoneyMqNetwork> {
        self.configs.get(name).map(|config| config.network())
    }
}

/// Network-specific configuration
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

/// Configuration for Solana Surfnet (local development network)
#[derive(Debug, Clone)]
pub struct SolanaSurfnetConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
    pub user_accounts: Vec<Recipient>,
}

/// Configuration for Solana Mainnet
#[derive(Debug, Clone)]
pub struct SolanaMainnetConfig {
    pub payment_recipient: Recipient,
    pub currencies: Vec<Currency>,
}
