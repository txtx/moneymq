use console::style;
use indexmap::IndexMap;
use moneymq_core::{gateway::NetworksConfig, validator::SolanaValidatorConfig};
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::x402::config::facilitator::{
    SurfnetRpcConfig, ValidatorNetworkConfig, ValidatorsConfig,
};
use moneymq_types::x402::{
    MoneyMqNetwork,
    config::{
        constants::DEFAULT_FACILITATOR_PORT,
        facilitator::{
            FacilitatorConfig, FacilitatorNetworkConfig, SolanaSurfnetFacilitatorConfig,
        },
    },
};
use solana_keypair::Signer;
use url::Url;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::{
    manifest::{
        Manifest,
        x402::{NetworkIdentifier, PaymentConfig},
    },
    service::{BillingNetworksMap, RunCommandError, ServiceCommand},
};

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct SandboxCommand {
    /// Port to run the server on
    #[arg(long, default_value = "8488")]
    pub port: u16,
}

impl ServiceCommand for SandboxCommand {
    const SANDBOX: bool = true;
    fn port(&self) -> u16 {
        self.port
    }

    fn billing_networks(
        &self,
        manifest: &Manifest,
    ) -> Result<
        IndexMap<String, (MoneyMqNetwork, Option<String>, Vec<String>)>,
        super::RunCommandError,
    > {
        let mut billing_networks = manifest
            .payments
            .iter()
            .flat_map(|(_name, payment_config)| match payment_config {
                PaymentConfig::X402(x402_config) => {
                    // Get networks from accepted config
                    x402_config
                        .accepted
                        .iter()
                        .map(|(network_id, network)| {
                            (
                                network_id.to_string(),
                                (
                                    MoneyMqNetwork::SolanaSurfnet,
                                    network.recipient.clone(),
                                    network.currencies.clone(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                }
            })
            .collect::<IndexMap<_, _>>();

        if billing_networks.is_empty() {
            billing_networks.insert(
                "solana".to_string(),
                (
                    MoneyMqNetwork::SolanaSurfnet,
                    None,                     // No payment recipient for default config
                    vec!["USDC".to_string()], // Default currency
                ),
            );
        }
        Ok(billing_networks)
    }

    fn networks_config(
        &self,
        billing_networks: BillingNetworksMap,
    ) -> Result<NetworksConfig, super::RunCommandError> {
        let networks_config = NetworksConfig::initialize(billing_networks, Self::SANDBOX)
            .map_err(RunCommandError::NetworksConfigInitializationError)?;
        Ok(networks_config)
    }

    async fn setup_facilitator(
        &self,
        payments: &IndexMap<String, PaymentConfig>,
        networks_config: &NetworksConfig,
    ) -> Result<(Url, String, ValidatorsConfig), RunCommandError> {
        let (facilitator_config, validators_config) = build_facilitator_config(payments)
            .await
            .map_err(RunCommandError::StartFacilitatorNetworks)?;

        // Only start local facilitator server in sandbox mode
        // This will generate the facilitator keypair if needed
        let (_facilitator_handle, facilitator_url, facilitator_pubkey) =
            start_facilitator_networks(facilitator_config, &validators_config, &networks_config)
                .await
                .map_err(RunCommandError::StartFacilitatorNetworks)?;

        println!();
        println!(
            "# {}{}{}{}{}",
            style("Payment API (protocol: ").dim(),
            style("x402").green(),
            style(", facilitator public key: ").dim(),
            style(facilitator_pubkey.to_string()).green(),
            style(")").dim()
        );
        println!(" {} {}supported", Self::get(), facilitator_url);
        println!(" {} {}verify", Self::post(), facilitator_url);
        println!(" {} {}settle", Self::post(), facilitator_url);

        networks_config
            .fund_accounts(&validators_config)
            .await
            .map_err(RunCommandError::FundLocalAccountsError)?;

        Ok((facilitator_url, facilitator_pubkey, validators_config))
    }
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type FacilitatorHandle = tokio::task::JoinHandle<Result<(), Error>>;

async fn start_facilitator_networks(
    mut facilitator_config: FacilitatorConfig,
    validators_config: &ValidatorsConfig,
    networks_config: &NetworksConfig,
) -> Result<(FacilitatorHandle, url::Url, String), String> {
    #[cfg(feature = "embedded_validator")]
    for (network_name, facilitator_network_config) in facilitator_config.networks.iter_mut() {
        match facilitator_network_config {
            FacilitatorNetworkConfig::SolanaSurfnet(surfnet_facilitator_config) => {
                use moneymq_types::x402::config::facilitator::ValidatorNetworkConfig;

                let network_config = networks_config
                    .configs
                    .get(network_name)
                    .and_then(|c| c.surfnet_config());
                let Some(ValidatorNetworkConfig::SolanaSurfnet(surfnet_rpc_config)) =
                    validators_config.networks.get(network_name)
                else {
                    continue;
                };

                // If the payer pubkey is set, we can assume it came from the env file, so it's already set
                // If the payer pubkey is not set, generate a deterministic one for sandbox
                if surfnet_facilitator_config.payer_pubkey.is_none() {
                    use sha2::{Digest, Sha256};
                    use solana_keypair::Keypair;

                    // Generate deterministic keypair from a fixed seed for sandbox
                    // This ensures the same fee payer address across restarts
                    let seed_phrase = "moneymq-sandbox-facilitator-fee-payer-v1";
                    let mut hasher = Sha256::new();
                    hasher.update(seed_phrase.as_bytes());
                    let seed = hasher.finalize();

                    let seed_array: [u8; 32] = seed[..32].try_into().unwrap();
                    let new_keypair = Keypair::new_from_array(seed_array);
                    surfnet_facilitator_config.payer_pubkey = Some(new_keypair.pubkey());
                    // It needs to be written to the env so Kora can pick it up.
                    // TODO: remove once Kora can accept Keypair directly
                    unsafe {
                        use moneymq_core::api::payment::SOLANA_KEYPAIR_ENV;

                        let value = new_keypair.to_base58_string();
                        std::env::set_var(SOLANA_KEYPAIR_ENV, value);
                    }
                }

                let validator_config = SolanaValidatorConfig {
                    rpc_config: surfnet_rpc_config.clone(),
                    facilitator_pubkey: surfnet_facilitator_config
                        .payer_pubkey
                        .expect("Facilitator pubkey should be initialized"),
                };

                let Some(_) =
                    moneymq_core::validator::start_surfpool(validator_config, network_config)
                        .map_err(|e| {
                            format!(
                                "Failed to start Solana Surfnet validator for network '{}': {}",
                                network_name, e
                            )
                        })?
                else {
                    continue;
                };
            }
            FacilitatorNetworkConfig::SolanaMainnet(_) => {
                // No local validator for mainnet
            }
        }
    }

    let url = facilitator_config.url.clone();

    // Extract the facilitator pubkey before the config is consumed
    let facilitator_pubkey = facilitator_config
        .get_facilitator_pubkey(&"solana".to_string())
        .expect("Facilitator pubkey should be initialized");

    let handle = moneymq_core::api::payment::start_facilitator(facilitator_config, true)
        .await
        .map_err(|e| format!("Failed to start facilitator: {e}"))?;

    Ok((handle, url, facilitator_pubkey))
}

async fn build_facilitator_config(
    payments: &IndexMap<String, PaymentConfig>,
) -> Result<(FacilitatorConfig, ValidatorsConfig), String> {
    let sandbox_x402_config = payments
        .iter()
        .filter_map(|(name, payment_config)| {
            match payment_config {
                PaymentConfig::X402(x402_config) => {
                    // Check if there's a "default" sandbox configuration with local facilitator
                    x402_config
                        .sandboxes
                        .get("default")
                        .map(|c| (name.clone(), c.clone()))
                }
            }
        })
        .collect::<Vec<_>>();

    if sandbox_x402_config.is_empty() {
        // Create default in-memory configuration
        let mut networks = std::collections::HashMap::new();
        networks.insert(
            NetworkIdentifier::Solana.to_string(),
            FacilitatorNetworkConfig::SolanaSurfnet(SolanaSurfnetFacilitatorConfig::default()),
        );

        return Ok((
            FacilitatorConfig {
                url: format!("http://localhost:{}", DEFAULT_FACILITATOR_PORT)
                    .parse::<Url>()
                    .expect("Failed to parse default facilitator URL"),
                networks,
            },
            ValidatorsConfig {
                networks: std::collections::HashMap::from([(
                    NetworkIdentifier::Solana.to_string(),
                    ValidatorNetworkConfig::SolanaSurfnet(SurfnetRpcConfig::default()),
                )]),
            },
        ));
    }

    if sandbox_x402_config.len() > 1 {
        eprintln!(
            "{} Multiple X402 sandbox networks found in manifest. Only the first local facilitator ({}) will be started.",
            style("Warning:").yellow(),
            sandbox_x402_config[0].0
        );
    }

    let sandbox_config = &sandbox_x402_config[0].1;
    let configs: (FacilitatorConfig, ValidatorsConfig) = sandbox_config.try_into()?;
    Ok(configs)
}
