use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use console::style;
use indexmap::IndexMap;
use moneymq_core::billing::BillingManager;
use moneymq_core::billing::BillingManagerError;
use moneymq_core::validator::SolanaValidatorConfig;
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::Meter;
use moneymq_types::Product;
use moneymq_types::x402::Network;
use moneymq_types::x402::config::facilitator::FacilitatorConfig;
use moneymq_types::x402::config::facilitator::FacilitatorNetworkConfig;
use solana_keypair::Signer;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::Context;
use crate::manifest::ProviderConfig;

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
    #[error("Catalog directory not found: {0}\nRun 'moneymq init' or 'moneymq catalog sync' first")]
    CatalogDirNotFound(String),
    #[error("Failed to read {} directory: {}", .0.display(), 1)]
    DirectoryReadError(PathBuf, std::io::Error),
    #[error("Failed to read directory entry: {0}")]
    DirectoryEntryReadError(std::io::Error),
    #[error("Failed to read file {}: {}", .0.display(), 1)]
    ReadFileError(PathBuf, std::io::Error),
    #[error("No products found in catalog directory")]
    NoProductsFound,
    #[error("Failed to start local facilitator networks: {0}")]
    StartFacilitatorNetworks(String),
    #[error("Failed configure billing settings: {0}")]
    BillingManagerInitializationError(BillingManagerError),
    #[error("Failed to fund local accounts: {0}")]
    FundLocalAccountsError(String),
    #[error("Failed to start provider server: {0}")]
    ProviderStartError(Box<dyn std::error::Error>),
}

impl RunCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
        println!();
        println!("{}{}", style("Money").white(), style("MQ").green());
        println!("{}", style("Starting provider server").dim());
        println!();

        // Get catalog path from first Stripe provider (or default to "billing/catalog/v1")
        let catalog_path = ctx
            .manifest
            .providers
            .values()
            .find_map(|p| p.stripe_config())
            .map(|c| c.catalog_path.as_str())
            .unwrap_or("billing/catalog/v1");

        // Load products from catalog directory
        let catalog_dir = ctx.manifest_path.join(catalog_path);

        if !catalog_dir.exists() {
            return Err(RunCommandError::CatalogDirNotFound(
                catalog_dir.display().to_string(),
            ));
        }

        print!("{} ", style("Loading products").dim());

        let mut products = Vec::new();
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

        if products.is_empty() {
            return Err(RunCommandError::NoProductsFound);
        }

        println!(
            "{}",
            style(format!("✓ {} products", products.len())).green()
        );

        // Load meters from metering directory (replace "catalog" with "metering" in path)
        let metering_path = catalog_path.replace("/catalog/", "/metering/");
        let metering_dir = ctx.manifest_path.join(metering_path);
        let mut meters = Vec::new();

        if metering_dir.exists() {
            print!("{} ", style("Loading meters").dim());

            let meter_entries = fs::read_dir(&metering_dir)
                .map_err(|e| RunCommandError::DirectoryReadError(metering_dir, e))?;

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

            println!("{}", style(format!("✓ {} meters", meters.len())).green());
        }

        println!();

        let mode = if self.sandbox {
            "sandbox"
        } else {
            "production"
        };
        println!("{} {}", style("Mode").dim(), mode);
        println!("{} {}", style("Port").dim(), self.port);
        println!();

        println!("{}", style("Endpoints").dim());
        println!("  GET http://localhost:{}/v1/products", self.port);
        println!("  GET http://localhost:{}/v1/prices", self.port);
        println!("  GET http://localhost:{}/v1/billing/meters", self.port);
        println!("  GET http://localhost:{}/health", self.port);
        println!();

        println!(
            "{}",
            style(format!(
                "Set STRIPE_API_BASE=http://localhost:{}",
                self.port
            ))
            .dim()
        );
        println!();
        println!("{}", style("Press Ctrl+C to stop").dim());
        println!();

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let billing_networks = ctx
            .manifest
            .providers
            .iter()
            .filter_map(|(_name, provider)| {
                provider.x402_config().and_then(|config| {
                    if self.sandbox {
                        config
                            .sandboxes
                            .get("default")
                            .map(|config| config.billing_networks.clone())
                    } else {
                        Some(config.billing_networks.clone())
                    }
                })
            })
            .flatten()
            .map(|(name, config)| {
                (
                    name.clone(),
                    (
                        config.network().clone(),
                        config.payment_recipient().clone(),
                        config.currencies().clone(),
                    ),
                )
            })
            .collect::<IndexMap<_, _>>();

        let billing_manager = BillingManager::initialize(billing_networks)
            .await
            .map_err(RunCommandError::BillingManagerInitializationError)?;

        // Only start local facilitator server in sandbox mode
        let handles = if self.sandbox {
            start_facilitator_networks(&ctx.manifest.providers, &billing_manager)
                .await
                .map_err(|e| RunCommandError::StartFacilitatorNetworks(e))?
        } else {
            None
        };

        // Build facilitator config for provider server
        let Some((_facilitator_handle, local_validator_ctx, facilitator_url)) = handles else {
            panic!("Facilitator must be started in sandbox mode");
        };

        let local_validator_rpc_urls = local_validator_ctx
            .iter()
            .map(|(network, (_handle, url))| (network.clone(), url.clone()))
            .collect::<IndexMap<_, _>>();
        billing_manager
            .fund_accounts(&local_validator_rpc_urls)
            .await
            .map_err(RunCommandError::FundLocalAccountsError)?;

        // Start the server
        moneymq_core::provider::start_provider(
            products,
            meters,
            facilitator_url,
            self.port,
            self.sandbox,
            billing_manager,
        )
        .await
        .map_err(RunCommandError::ProviderStartError)?;

        Ok(())
    }
}

async fn build_facilitator_config(
    providers: &HashMap<String, ProviderConfig>,
) -> Result<FacilitatorConfig, String> {
    let sandbox_x402_config = providers
        .iter()
        .filter_map(|(name, provider)| {
            provider.x402_config().and_then(|config| {
                // Check if there's a "default" sandbox configuration with local facilitator
                config
                    .sandboxes
                    .get("default")
                    .map(|c| (name.clone(), c.clone()))
            })
        })
        .collect::<Vec<_>>();

    if sandbox_x402_config.is_empty() {
        return Err("No X402 sandbox configuration found in manifest".to_string());
    }

    if sandbox_x402_config.len() > 1 {
        eprintln!(
            "{} Multiple X402 sandbox providers found in manifest. Only the first local facilitator ({}) will be started.",
            style("Warning:").yellow(),
            sandbox_x402_config[0].0
        );
    }

    let facilitator_config_file = &sandbox_x402_config[0].1.facilitator;
    let facilitator_config: FacilitatorConfig = facilitator_config_file.try_into()?;
    Ok(facilitator_config)
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type FacilitatorHandle = tokio::task::JoinHandle<Result<(), Error>>;
type ValidatorData = IndexMap<Network, (std::process::Child, url::Url)>;

async fn start_facilitator_networks(
    providers: &HashMap<String, ProviderConfig>,
    billing_manager: &BillingManager,
) -> Result<Option<(FacilitatorHandle, ValidatorData, url::Url)>, String> {
    // Build facilitator config for starting the facilitator
    let facilitator_config = build_facilitator_config(providers).await?;

    let mut local_validator_handles = IndexMap::new();
    for (network_name, network_config) in facilitator_config.networks.iter() {
        match network_config {
            FacilitatorNetworkConfig::SolanaSurfnet(surfnet_config) => {
                let billing_config = billing_manager
                    .configs
                    .get(network_name)
                    .and_then(|c| c.surfnet_config());

                let validator_config = SolanaValidatorConfig {
                    rpc_api_url: surfnet_config.rpc_url.clone(),
                    facilitator_pubkey: surfnet_config.payer_keypair.pubkey(),
                };

                let Some(handle) = moneymq_core::validator::start_local_solana_validator(
                    validator_config,
                    billing_config,
                )
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
                    .insert(Network::Solana, (handle, surfnet_config.rpc_url.clone()));
            }
            FacilitatorNetworkConfig::SolanaMainnet(_) => {
                // No local validator for mainnet
            }
        }
    }

    let url = facilitator_config.url.clone();
    let handle = moneymq_core::facilitator::start_facilitator(facilitator_config)
        .await
        .map_err(|e| format!("Failed to start facilitator: {e}"))?;

    Ok(Some((handle, local_validator_handles, url)))
}
