use console::style;
use indexmap::IndexMap;
use moneymq_core::{
    api::{NetworksConfig, payment::PaymentApiConfig},
    validator::SolanaValidatorConfig,
};
use moneymq_types::x402::{
    MoneyMqNetwork,
    config::{
        constants::DEFAULT_MONEYMQ_PORT,
        facilitator::{
            FacilitatorConfig, FacilitatorNetworkConfig, SolanaSurfnetFacilitatorConfig,
            SurfnetRpcConfig, ValidatorNetworkConfig, ValidatorsConfig,
        },
    },
};
use solana_keypair::Signer;
use url::Url;

use crate::{
    manifest::{Chain, EnvironmentConfig, Manifest, PaymentsConfig, SandboxEnvironment},
    service::{PaymentNetworksMap, RunCommandError, ServiceCommand},
};

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct RunCommand {
    /// Environment to run (e.g., "sandbox", "production")
    /// If not specified, uses "sandbox" if available, otherwise the first environment
    #[arg(default_value = "sandbox")]
    pub environment: String,

    /// Port to run the server on (overrides environment config)
    #[arg(long)]
    pub port: Option<u16>,

    /// Log level (error, warn, info, debug, trace). If not set, logging is disabled.
    #[arg(long)]
    pub log_level: Option<String>,
}

impl RunCommand {
    /// Get the environment config from the manifest
    pub fn get_environment<'a>(&self, manifest: &'a Manifest) -> Option<&'a EnvironmentConfig> {
        manifest.get_environment(&self.environment)
    }

    /// Check if the selected environment is a sandbox
    pub fn is_sandbox(&self, manifest: &Manifest) -> bool {
        // If environment is "sandbox" and not explicitly configured, it's still a sandbox
        if self.environment == "sandbox" {
            return self
                .get_environment(manifest)
                .map(|env| matches!(env, EnvironmentConfig::Sandbox(_)))
                .unwrap_or(true); // Default to true for sandbox
        }

        self.get_environment(manifest)
            .map(|env| matches!(env, EnvironmentConfig::Sandbox(_)))
            .unwrap_or(false)
    }

    /// Get the port, preferring CLI arg over environment config
    pub fn get_port(&self, manifest: &Manifest) -> u16 {
        if let Some(port) = self.port {
            return port;
        }

        self.get_environment(manifest)
            .map(|env| match env {
                EnvironmentConfig::Sandbox(e) => e.port,
                EnvironmentConfig::SelfHosted(e) => e.port,
                EnvironmentConfig::CloudHosted(_) => DEFAULT_MONEYMQ_PORT,
            })
            .unwrap_or(DEFAULT_MONEYMQ_PORT)
    }
}

impl ServiceCommand for RunCommand {
    fn environment_name(&self) -> &str {
        &self.environment
    }

    fn port(&self, manifest: &Manifest) -> u16 {
        self.get_port(manifest)
    }

    fn is_sandbox(&self, manifest: &Manifest) -> bool {
        self.is_sandbox(manifest)
    }

    fn log_level(&self) -> Option<&str> {
        self.log_level.as_deref()
    }

    fn payment_networks(
        &self,
        manifest: &Manifest,
    ) -> Result<PaymentNetworksMap, super::RunCommandError> {
        // Get network info from payments config
        let chain = manifest.payments.chain();
        let stablecoins = manifest.payments.stablecoins().to_vec();

        // Get recipient from the selected environment
        let recipient = self.get_environment(manifest).and_then(|env| match env {
            EnvironmentConfig::Sandbox(e) => e.network.recipient.clone(),
            EnvironmentConfig::SelfHosted(e) => e.network.recipient.clone(),
            EnvironmentConfig::CloudHosted(_) => None,
        });

        let network_id = match chain {
            Chain::Solana => "solana".to_string(),
        };

        let mut payment_networks = IndexMap::new();
        payment_networks.insert(
            network_id,
            (
                MoneyMqNetwork::SolanaSurfnet,
                recipient,
                if stablecoins.is_empty() {
                    vec!["USDC".to_string()]
                } else {
                    stablecoins
                },
            ),
        );

        Ok(payment_networks)
    }

    fn networks_config(
        &self,
        manifest: &Manifest,
        payment_networks: PaymentNetworksMap,
    ) -> Result<NetworksConfig, super::RunCommandError> {
        let is_sandbox = self.is_sandbox(manifest);
        let networks_config = NetworksConfig::initialize(payment_networks, is_sandbox)
            .map_err(RunCommandError::NetworksConfigInitializationError)?;
        Ok(networks_config)
    }

    async fn setup_payment_api(
        &self,
        _payments: &PaymentsConfig,
        environment: &EnvironmentConfig,
        networks_config: &NetworksConfig,
        port: u16,
    ) -> Result<(Url, String, ValidatorsConfig, PaymentApiConfig), RunCommandError> {
        match environment {
            EnvironmentConfig::Sandbox(sandbox) => {
                self.setup_sandbox_payment_api(sandbox, networks_config, port)
                    .await
            }
            EnvironmentConfig::SelfHosted(_) => Err(RunCommandError::StartPaymentApi(
                "SelfHosted environment not yet implemented".to_string(),
            )),
            EnvironmentConfig::CloudHosted(_) => Err(RunCommandError::StartPaymentApi(
                "CloudHosted environment not yet implemented".to_string(),
            )),
        }
    }
}

impl RunCommand {
    async fn setup_sandbox_payment_api(
        &self,
        sandbox: &SandboxEnvironment,
        networks_config: &NetworksConfig,
        port: u16,
    ) -> Result<(Url, String, ValidatorsConfig, PaymentApiConfig), RunCommandError> {
        let (facilitator_config, validators_config) =
            build_sandbox_payment_api_config(sandbox, port)
                .map_err(RunCommandError::StartPaymentApi)?;

        // Setup local validator and create payment API state
        let (payment_api_url, facilitator_pubkey, payment_api_state) =
            setup_payment_api_networks(facilitator_config, &validators_config, networks_config)
                .await
                .map_err(RunCommandError::StartPaymentApi)?;

        println!();
        println!(
            "# {}{}{}{}{}",
            style("Payment API (protocol: ").dim(),
            style("x402").green(),
            style(", facilitator: ").dim(),
            style(facilitator_pubkey.to_string()).green(),
            style(")").dim()
        );
        println!(
            " {} http://localhost:{}/payment/v1/supported",
            Self::get(),
            port
        );
        println!(
            " {} http://localhost:{}/payment/v1/verify",
            Self::post(),
            port
        );
        println!(
            " {} http://localhost:{}/payment/v1/settle",
            Self::post(),
            port
        );
        println!(" {} http://localhost:{}/events", Self::get(), port);

        networks_config
            .fund_accounts(&validators_config)
            .await
            .map_err(RunCommandError::FundLocalAccountsError)?;

        Ok((
            payment_api_url,
            facilitator_pubkey,
            validators_config,
            payment_api_state,
        ))
    }
}

/// Setup the payment API networks (starts validators, creates facilitator state)
async fn setup_payment_api_networks(
    mut facilitator_config: FacilitatorConfig,
    validators_config: &ValidatorsConfig,
    networks_config: &NetworksConfig,
) -> Result<(url::Url, String, PaymentApiConfig), String> {
    #[cfg(feature = "embedded_validator")]
    for (network_name, facilitator_network_config) in facilitator_config.networks.iter_mut() {
        match facilitator_network_config {
            FacilitatorNetworkConfig::SolanaSurfnet(surfnet_config) => {
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

                // If the payer pubkey is set, we can assume it came from the env file
                // If not set, generate a deterministic one for sandbox
                if surfnet_config.payer_pubkey.is_none() {
                    use sha2::{Digest, Sha256};
                    use solana_keypair::Keypair;

                    // Generate deterministic keypair from a fixed seed for sandbox
                    let seed_phrase = "moneymq-sandbox-payment-api-fee-payer-v1";
                    let mut hasher = Sha256::new();
                    hasher.update(seed_phrase.as_bytes());
                    let seed = hasher.finalize();

                    let seed_array: [u8; 32] = seed[..32].try_into().unwrap();
                    let new_keypair = Keypair::new_from_array(seed_array);
                    surfnet_config.payer_pubkey = Some(new_keypair.pubkey());
                    // Write to env so Kora can pick it up
                    unsafe {
                        use moneymq_core::api::payment::SOLANA_KEYPAIR_ENV;
                        let value = new_keypair.to_base58_string();
                        std::env::set_var(SOLANA_KEYPAIR_ENV, value);
                    }
                }

                let validator_config = SolanaValidatorConfig {
                    rpc_config: surfnet_rpc_config.clone(),
                    facilitator_pubkey: surfnet_config
                        .payer_pubkey
                        .expect("Payment API pubkey should be initialized"),
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
        .get_facilitator_pubkey("solana")
        .expect("Facilitator pubkey should be initialized");

    // Create the payment API state
    let payment_api_state = moneymq_core::api::payment::create_payment_api_config(
        facilitator_config,
        validators_config.clone(),
        true,
    )
    .await
    .map_err(|e| format!("Failed to create payment API state: {e}"))?;

    Ok((url, facilitator_pubkey, payment_api_state))
}

/// Build payment API config from sandbox environment
fn build_sandbox_payment_api_config(
    sandbox: &SandboxEnvironment,
    port: u16,
) -> Result<(FacilitatorConfig, ValidatorsConfig), String> {
    let mut networks = std::collections::HashMap::new();
    networks.insert(
        "solana".to_string(),
        FacilitatorNetworkConfig::SolanaSurfnet(SolanaSurfnetFacilitatorConfig::default()),
    );

    // Build RPC config from sandbox network settings
    let rpc_url = format!(
        "http://{}:{}",
        sandbox.network.binding_address, sandbox.network.rpc_port
    )
    .parse::<Url>()
    .map_err(|e| format!("Failed to parse RPC URL: {}", e))?;

    let surfnet_rpc_config = SurfnetRpcConfig {
        rpc_url,
        bind_host: Some(sandbox.network.binding_address.clone()),
        rpc_port: Some(sandbox.network.rpc_port),
        ws_port: Some(sandbox.network.ws_port),
    };

    let payment_api_url = format!("http://{}:{}", sandbox.binding_address, port)
        .parse::<Url>()
        .map_err(|e| format!("Failed to parse payment API URL: {}", e))?;

    Ok((
        FacilitatorConfig {
            url: payment_api_url,
            networks,
        },
        ValidatorsConfig {
            networks: std::collections::HashMap::from([(
                "solana".to_string(),
                ValidatorNetworkConfig::SolanaSurfnet(surfnet_rpc_config),
            )]),
        },
    ))
}
