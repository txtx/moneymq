use std::{fs, path::PathBuf};

use console::style;
use indexmap::IndexMap;
use moneymq_core::{
    billing::{BillingManager, BillingManagerError},
    validator::SolanaValidatorConfig,
};
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::Meter;
use moneymq_types::{
    Product,
    x402::{
        MoneyMqNetwork, Network,
        config::facilitator::{FacilitatorConfig, FacilitatorNetworkConfig},
    },
};
use solana_keypair::Signer;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::Context;
use crate::manifest::x402::X402Config;

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
        println!();

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

        // Load meters from {catalog_path}/meters directory
        let meters_dir = ctx.manifest_path.join(catalog_base_path).join("meters");
        let mut meters = Vec::new();

        if meters_dir.exists() {
            print!("{} ", style("Loading meters").dim());

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

            println!("{}", style(format!("✓ {} meters", meters.len())).green());
        }

        println!();

        let get = style("  GET").yellow();
        let post = style(" POST").magenta();

        println!(
            "# {}",
            style("catalog endpoints (compatible with Stripe)").dim()
        );
        println!(" {} http://localhost:{}/v1/products", get, self.port);
        println!(" {} http://localhost:{}/v1/billing/meters", post, self.port);
        println!(" {} http://localhost:{}/v1/billing/meters", post, self.port);
        println!();

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let mut billing_networks = ctx
            .manifest
            .networks
            .iter()
            .filter_map(|(_name, x402_config)| {
                if self.sandbox {
                    x402_config
                        .sandboxes
                        .get("default")
                        .map(|config| config.billing_networks.clone())
                } else {
                    Some(x402_config.billing_networks.clone())
                }
            })
            .flatten()
            .map(|(name, config)| {
                (
                    name.clone(),
                    (
                        config.network().clone(),
                        config.payment_recipient().clone(),
                        config.currencies().clone(),
                        config.user_accounts().clone(),
                    ),
                )
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

        let billing_manager = BillingManager::initialize(billing_networks, self.sandbox)
            .await
            .map_err(RunCommandError::BillingManagerInitializationError)?;

        // Build facilitator config once for sandbox mode
        let (facilitator_pubkey, handles) = if self.sandbox {
            let config = build_facilitator_config(&ctx.manifest.networks)
                .await
                .map_err(RunCommandError::StartFacilitatorNetworks)?;
            let pubkey = config.networks.values().next().and_then(|net| {
                match net {
                    moneymq_types::x402::config::facilitator::FacilitatorNetworkConfig::SolanaSurfnet(cfg) => {
                        Some(cfg.payer_keypair.pubkey().to_string())
                    }
                    _ => None,
                }
            });

            println!("# {}", style("x402 endpoints").dim());
            println!(" {} {}supported", get, config.url);
            println!(" {} {}verify", post, config.url);
            println!(" {} {}settle", post, config.url);
            println!();

            // Only start local facilitator server in sandbox mode
            let handles = start_facilitator_networks(config, &billing_manager)
                .await
                .map_err(RunCommandError::StartFacilitatorNetworks)?;
            (pubkey, handles)
        } else {
            (None, None)
        };

        println!();
        println!("{}", style("Press Ctrl+C to stop").dim());

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
        moneymq_core::provider::start_provider(
            products,
            meters,
            facilitator_url,
            self.port,
            self.sandbox,
            billing_manager,
            catalog_path,
            catalog_name,
            catalog_description,
            facilitator_pubkey,
            validator_rpc_url,
        )
        .await
        .map_err(RunCommandError::ProviderStartError)?;

        Ok(())
    }
}

async fn build_facilitator_config(
    networks: &IndexMap<String, X402Config>,
) -> Result<FacilitatorConfig, String> {
    let sandbox_x402_config = networks
        .iter()
        .filter_map(|(name, x402_config)| {
            // Check if there's a "default" sandbox configuration with local facilitator
            x402_config
                .sandboxes
                .get("default")
                .map(|c| (name.clone(), c.clone()))
        })
        .collect::<Vec<_>>();

    if sandbox_x402_config.is_empty() {
        // Create default in-memory configuration
        use moneymq_types::x402::config::facilitator::SolanaSurfnetFacilitatorConfig;
        use solana_keypair::Keypair;
        use url::Url;

        let mut networks = std::collections::HashMap::new();
        networks.insert(
            "solana".to_string(),
            FacilitatorNetworkConfig::SolanaSurfnet(SolanaSurfnetFacilitatorConfig {
                rpc_url: "http://127.0.0.1:8899"
                    .parse::<Url>()
                    .expect("Failed to parse default RPC URL"),
                payer_keypair: Keypair::new(),
            }),
        );

        return Ok(FacilitatorConfig {
            url: crate::manifest::x402::DEFAULT_LOCAL_FACILITATOR_URL
                .parse::<Url>()
                .expect("Failed to parse default facilitator URL"),
            networks,
            api_token: None,
        });
    }

    if sandbox_x402_config.len() > 1 {
        eprintln!(
            "{} Multiple X402 sandbox networks found in manifest. Only the first local facilitator ({}) will be started.",
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
    facilitator_config: FacilitatorConfig,
    billing_manager: &BillingManager,
) -> Result<Option<(FacilitatorHandle, ValidatorData, url::Url)>, String> {
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
