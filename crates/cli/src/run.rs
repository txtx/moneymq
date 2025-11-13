use std::{fs, path::PathBuf};

use console::style;
use indexmap::IndexMap;
use moneymq_core::{
    billing::{NetworksConfig, NetworksConfigError},
    validator::SolanaValidatorConfig,
};
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::Meter;
use moneymq_types::{
    Product,
    x402::{
        MoneyMqNetwork, Network,
        config::{
            constants::DEFAULT_FACILITATOR_PORT,
            facilitator::{
                FacilitatorConfig, FacilitatorNetworkConfig, SolanaSurfnetFacilitatorConfig,
            },
        },
    },
};
use solana_keypair::Signer;
use url::Url;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::{
    Context,
    manifest::x402::{
        FacilitatorConfig as ManifestFacilitatorConfig, NetworkIdentifier, PaymentConfig,
    },
};

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct RunCommand {
    /// Port to run the server on
    #[arg(long, default_value = "8488")]
    pub port: u16,

    /// Use sandbox mode (serve sandbox external IDs)
    #[arg(long)]
    pub sandbox: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RunCommandError {
    #[error("Failed to read {} directory: {}", .0.display(), 1)]
    DirectoryReadError(PathBuf, std::io::Error),
    #[error("Failed to read directory entry: {0}")]
    DirectoryEntryReadError(std::io::Error),
    #[error("Failed to read file {}: {}", .0.display(), 1)]
    ReadFileError(PathBuf, std::io::Error),
    #[error("Failed to start local facilitator networks: {0}")]
    StartFacilitatorNetworks(String),
    #[error("Failed configure billing settings: {0}")]
    NetworksConfigInitializationError(NetworksConfigError),
    #[error("Failed to fund local accounts: {0}")]
    FundLocalAccountsError(String),
    #[error("Failed to start provider server: {0}")]
    ProviderStartError(Box<dyn std::error::Error>),
}

impl RunCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
        // Get catalog path from first catalog (or default to "billing/v1")
        let catalog_base_path = ctx
            .manifest
            .catalogs
            .values()
            .next()
            .map(|c| c.catalog_path.as_str())
            .unwrap_or("billing/v1");

        // Load products from {catalog_path}/products directory
        let catalog_dir = ctx.manifest_path.join(catalog_base_path).join("products");

        print!("{} ", style("Loading products").dim());

        let mut products = Vec::new();

        if catalog_dir.exists() {
            let entries = fs::read_dir(&catalog_dir)
                .map_err(|e| RunCommandError::DirectoryReadError(catalog_dir, e))?;

            for entry in entries {
                let entry = entry.map_err(RunCommandError::DirectoryEntryReadError)?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                    let content = fs::read_to_string(&path)
                        .map_err(|e| RunCommandError::ReadFileError(path.clone(), e))?;

                    match serde_yml::from_str::<Product>(&content) {
                        Ok(product) => {
                            products.push(product);
                        }
                        Err(e) => {
                            eprintln!(
                                "\n{} Failed to parse {}: {}",
                                style("✗").red(),
                                path.display(),
                                e
                            );
                            eprintln!("  {}", style("Skipping this file").dim());
                        }
                    }
                }
            }
        }

        if products.is_empty() {
            println!("{}", style("⚠ No products found").yellow());
        } else {
            println!(
                "{}",
                style(format!("✓ {} products", products.len())).green()
            );
        }

        // Load meters from {catalog_path}/meters directory
        let meters_dir = ctx.manifest_path.join(catalog_base_path).join("meters");
        let mut meters = Vec::new();

        print!("{} ", style("Loading meters").dim());

        if !meters_dir.exists() {
            println!("{}", style("⚠ Meters directory not found").yellow());
        } else {
            let meter_entries = fs::read_dir(&meters_dir)
                .map_err(|e| RunCommandError::DirectoryReadError(meters_dir, e))?;

            for entry in meter_entries {
                let entry = entry.map_err(RunCommandError::DirectoryEntryReadError)?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                    let content = fs::read_to_string(&path)
                        .map_err(|e| RunCommandError::ReadFileError(path.clone(), e))?;

                    match serde_yml::from_str::<Meter>(&content) {
                        Ok(meter) => {
                            meters.push(meter);
                        }
                        Err(e) => {
                            eprintln!(
                                "\n{} Failed to parse {}: {}",
                                style("✗").red(),
                                path.display(),
                                e
                            );
                            eprintln!("  {}", style("Skipping this file").dim());
                        }
                    }
                }
            }

            if meters.is_empty() {
                println!("{}", style("⚠ No meters found").yellow());
            } else {
                println!("{}", style(format!("✓ {} meters", meters.len())).green());
            }
        }

        println!();

        let get = style("  GET").yellow();
        let post = style(" POST").magenta();

        println!(
            "# {}{}{}",
            style("Catalog & Billing API (schema: ").dim(),
            style("stripe").green(),
            style(")").dim()
        );
        println!(" {} http://localhost:{}/v1/products", get, self.port);
        println!(" {} http://localhost:{}/v1/billing/meters", post, self.port);
        println!(" {}", style(" ...").dim());

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let mut billing_networks = ctx
            .manifest
            .payments
            .iter()
            .flat_map(|(_name, payment_config)| match payment_config {
                PaymentConfig::X402(x402_config) => {
                    if self.sandbox {
                        // Get networks from sandbox config
                        x402_config
                            .sandboxes
                            .get("default")
                            .and_then(|sandbox| match &sandbox.facilitator {
                                ManifestFacilitatorConfig::Embedded(config) => Some(
                                    config
                                        .supported
                                        .iter()
                                        .map(|(network_id, network)| {
                                            (
                                                network_id.to_string(),
                                                (
                                                    MoneyMqNetwork::SolanaSurfnet,
                                                    network.recipient.clone(),
                                                    network.currencies.clone(),
                                                    network.user_accounts.clone(),
                                                ),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                                ManifestFacilitatorConfig::ServiceUrl { .. } => None,
                            })
                            .unwrap_or_default()
                    } else {
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
                                        vec![],
                                    ),
                                )
                            })
                            .collect::<Vec<_>>()
                    }
                }
            })
            .collect::<IndexMap<_, _>>();

        // If no billing networks configured in sandbox mode, create default
        if self.sandbox && billing_networks.is_empty() {
            billing_networks.insert(
                "solana".to_string(),
                (
                    MoneyMqNetwork::SolanaSurfnet,
                    None,                     // No payment recipient for default config
                    vec!["USDC".to_string()], // Default currency
                    vec![],                   // No user accounts for default config
                ),
            );
        }

        let networks_config = NetworksConfig::initialize(billing_networks, self.sandbox)
            .map_err(RunCommandError::NetworksConfigInitializationError)?;

        // Build facilitator config once for sandbox mode
        let (facilitator_pubkey, handles) = if self.sandbox {
            let config = build_facilitator_config(&ctx.manifest.payments)
                .await
                .map_err(RunCommandError::StartFacilitatorNetworks)?;

            let facilitator_pubkey =
                config.get_facilitator_pubkey(&NetworkIdentifier::Solana.to_string());

            println!();
            println!(
                "# {}{}{}",
                style("Payment API (protocol: ").dim(),
                style("x402").green(),
                style(")").dim()
            );
            println!(" {} {}supported", get, config.url);
            println!(" {} {}verify", post, config.url);
            println!(" {} {}settle", post, config.url);

            // Only start local facilitator server in sandbox mode
            let handles = start_facilitator_networks(config, &networks_config, self.sandbox)
                .await
                .map_err(RunCommandError::StartFacilitatorNetworks)?;
            (facilitator_pubkey, handles)
        } else {
            (None, None)
        };

        println!();
        println!(
            "{}{} {}: {} - {}",
            style("Money").white(),
            style("MQ").green(),
            style("Studio:").white(),
            style(format!("http://localhost:{}", self.port)).cyan(),
            style("Press Ctrl+C to stop").dim()
        );
        println!();

        let Some((_facilitator_handle, local_validator_ctx, facilitator_url)) = handles else {
            panic!("Facilitator must be started in sandbox mode");
        };

        let local_validator_rpc_urls = local_validator_ctx
            .iter()
            .map(|(network, url)| (network.clone(), url.clone()))
            .collect::<IndexMap<_, _>>();
        networks_config
            .fund_accounts(&local_validator_rpc_urls)
            .await
            .map_err(RunCommandError::FundLocalAccountsError)?;

        // Get the first catalog name and description (for branding assets)
        let (catalog_name, catalog_description, catalog_path) = ctx
            .manifest
            .catalogs
            .iter()
            .next()
            .map(|(name, config)| {
                (
                    Some(name.clone()),
                    config.description.clone(),
                    PathBuf::from(&config.catalog_path),
                )
            })
            .unwrap_or((None, None, ctx.manifest_path.clone()));

        // Use the actual local validator RPC URL from the running validator
        let validator_rpc_url = if self.sandbox {
            local_validator_rpc_urls.values().next().cloned()
        } else {
            None
        };

        // Start the server
        moneymq_core::catalog::start_provider(
            products,
            meters,
            facilitator_url,
            self.port,
            self.sandbox,
            networks_config,
            catalog_path,
            catalog_name,
            catalog_description,
            facilitator_pubkey,
            validator_rpc_url,
            None, // kora_config
            None, // signer_pool
        )
        .await
        .map_err(RunCommandError::ProviderStartError)?;

        Ok(())
    }
}

async fn build_facilitator_config(
    payments: &IndexMap<String, PaymentConfig>,
) -> Result<FacilitatorConfig, String> {
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

        return Ok(FacilitatorConfig {
            url: format!("http://localhost:{}", DEFAULT_FACILITATOR_PORT)
                .parse::<Url>()
                .expect("Failed to parse default facilitator URL"),
            networks,
        });
    }

    if sandbox_x402_config.len() > 1 {
        eprintln!(
            "{} Multiple X402 sandbox networks found in manifest. Only the first local facilitator ({}) will be started.",
            style("Warning:").yellow(),
            sandbox_x402_config[0].0
        );
    }

    let sandbox_config = &sandbox_x402_config[0].1;
    let facilitator_config: FacilitatorConfig = sandbox_config.try_into()?;
    Ok(facilitator_config)
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type FacilitatorHandle = tokio::task::JoinHandle<Result<(), Error>>;
type ValidatorData = IndexMap<Network, url::Url>;

async fn start_facilitator_networks(
    facilitator_config: FacilitatorConfig,
    networks_config: &NetworksConfig,
    sandbox: bool,
) -> Result<Option<(FacilitatorHandle, ValidatorData, url::Url)>, String> {
    let mut local_validator_handles: IndexMap<Network, Url> = IndexMap::new();
    #[cfg(feature = "embedded_validator")]
    for (network_name, network_config) in facilitator_config.networks.iter() {
        match network_config {
            FacilitatorNetworkConfig::SolanaSurfnet(surfnet_config) => {
                let network_config = networks_config
                    .configs
                    .get(network_name)
                    .and_then(|c| c.surfnet_config());

                let validator_config = SolanaValidatorConfig {
                    rpc_config: surfnet_config.rpc_config.clone(),
                    facilitator_pubkey: surfnet_config.payer_keypair.pubkey(),
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
                local_validator_handles
                    .insert(Network::Solana, surfnet_config.rpc_config.rpc_url.clone());
            }
            FacilitatorNetworkConfig::SolanaMainnet(_) => {
                // No local validator for mainnet
            }
        }
    }

    let url = facilitator_config.url.clone();
    let handle = moneymq_core::facilitator::start_facilitator(
        facilitator_config,
        networks_config.clone(),
        sandbox,
    )
    .await
    .map_err(|e| format!("Failed to start facilitator: {e}"))?;

    Ok(Some((handle, local_validator_handles, url)))
}
