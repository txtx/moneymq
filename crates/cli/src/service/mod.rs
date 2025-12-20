use std::{fs, path::PathBuf};

use console::{StyledObject, style};
use indexmap::IndexMap;
use moneymq_core::api::{NetworksConfig, NetworksConfigError, payment::FacilitatorState};
use moneymq_types::{
    Meter, Product,
    x402::{MoneyMqNetwork, config::facilitator::ValidatorsConfig},
};
use url::Url;

use crate::{
    Context,
    manifest::{EnvironmentConfig, Manifest, PaymentsConfig},
};

mod run;
mod sandbox;

pub use run::RunCommand;
pub use sandbox::SandboxCommand;

#[derive(Debug, thiserror::Error)]
pub enum RunCommandError {
    #[error("Failed to read {} directory: {}", .0.display(), 1)]
    DirectoryReadError(PathBuf, std::io::Error),
    #[error("Failed to read directory entry: {0}")]
    DirectoryEntryReadError(std::io::Error),
    #[error("Failed to read file {}: {}", .0.display(), 1)]
    ReadFileError(PathBuf, std::io::Error),
    #[error("Failed to start payment API: {0}")]
    StartPaymentApi(String),
    #[error("Failed to configure payment networks: {0}")]
    NetworksConfigInitializationError(NetworksConfigError),
    #[error("Failed to fund local accounts: {0}")]
    FundLocalAccountsError(String),
    #[error("Failed to start provider server: {0}")]
    ProviderStartError(Box<dyn std::error::Error>),
    #[error("No payment networks configured")]
    NoPaymentNetworksConfigured,
    #[error("Environment '{0}' not found in manifest")]
    EnvironmentNotFound(String),
}

/// Map of payment network configurations: network_id -> (network_type, recipient, stablecoins)
type PaymentNetworksMap = IndexMap<String, (MoneyMqNetwork, Option<String>, Vec<String>)>;

pub trait ServiceCommand {
    fn get() -> StyledObject<&'static str> {
        style("  GET").yellow()
    }

    fn post() -> StyledObject<&'static str> {
        style(" POST").magenta()
    }

    /// Returns the environment name to use
    fn environment_name(&self) -> &str;

    /// Returns the port to use
    fn port(&self, manifest: &Manifest) -> u16;

    /// Returns true if running in sandbox mode
    fn is_sandbox(&self, manifest: &Manifest) -> bool;

    /// Build the payment networks map from the manifest
    fn payment_networks(&self, manifest: &Manifest) -> Result<PaymentNetworksMap, RunCommandError>;

    /// Build the networks configuration from payment networks
    fn networks_config(
        &self,
        manifest: &Manifest,
        payment_networks: PaymentNetworksMap,
    ) -> Result<NetworksConfig, RunCommandError>;

    /// Setup the payment API (facilitator) for the environment
    async fn setup_payment_api(
        &self,
        payments: &PaymentsConfig,
        environment: &EnvironmentConfig,
        networks_config: &NetworksConfig,
        port: u16,
    ) -> Result<(Url, String, ValidatorsConfig, Option<FacilitatorState>), RunCommandError>;

    fn load_catalog(
        &self,
        ctx: &Context,
        port: u16,
    ) -> Result<(Vec<Product>, Vec<Meter>), RunCommandError> {
        // Get catalog path from first catalog (or default to "billing/v1")
        let catalog_base_path = ctx
            .manifest
            .catalogs
            .values()
            .next()
            .map(|c| c.catalog_path.as_str())
            .unwrap_or("billing/v1");

        // Load products from {catalog_path}/products directory
        // Supports both legacy flat files and variant-based directories
        let catalog_dir = ctx.manifest_path.join(catalog_base_path).join("products");

        print!("{} ", style("Loading products").dim());

        let products: Vec<Product> =
            match crate::catalog::loader::load_products_from_directory(&catalog_dir) {
                Ok(products_map) => products_map.into_values().collect(),
                Err(e) => {
                    eprintln!("\n{} Failed to load products: {}", style("✗").red(), e);
                    Vec::new()
                }
            };

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

        println!(
            "# {}{}{}",
            style("Catalog API (schema: ").dim(),
            style("stripe").green(),
            style(")").dim()
        );

        println!(
            " {} http://localhost:{}/catalog/v1/products",
            Self::get(),
            port
        );
        println!(
            " {} http://localhost:{}/catalog/v1/billing/meters",
            Self::post(),
            port
        );
        println!(" {}", style(" ...").dim());

        Ok((products, meters))
    }

    async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
        let env_name = self.environment_name();
        let port = self.port(&ctx.manifest);
        let is_sandbox = self.is_sandbox(&ctx.manifest);

        // Get environment config, using default sandbox if "sandbox" is not explicitly configured
        let environment = ctx
            .manifest
            .get_environment(env_name)
            .cloned()
            .or_else(|| {
                // For sandbox, use default if not explicitly configured
                if env_name == "sandbox" {
                    Some(EnvironmentConfig::default())
                } else {
                    None
                }
            })
            .ok_or_else(|| RunCommandError::EnvironmentNotFound(env_name.to_string()))?;

        // If we're using the default manifest, there are no products/meters configured,
        // so the associated warnings are noisy and not helpful. Skip loading catalog in that case.
        let (products, meters) = if !ctx.is_default_manifest {
            self.load_catalog(ctx, port)?
        } else {
            (Vec::new(), Vec::new())
        };

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let payment_networks = self.payment_networks(&ctx.manifest)?;

        let networks_config = self.networks_config(&ctx.manifest, payment_networks)?;

        // Setup payment API
        let (_payment_api_url, facilitator_pubkey, validator_rpc_urls, payment_api_state) = self
            .setup_payment_api(&ctx.manifest.payments, &environment, &networks_config, port)
            .await?;

        println!();
        println!(
            "{}{} {}: {} - {}",
            style("Money").white(),
            style("MQ").green(),
            style("Studio:").white(),
            style(format!("http://localhost:{}", port)).cyan(),
            style("Press Ctrl+C to stop").dim()
        );
        println!();

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

        // Create the catalog provider state
        let payment_api_url = format!("http://localhost:{}/payment/v1/", port)
            .parse::<url::Url>()
            .expect("Failed to parse payment API URL");
        let catalog_state = moneymq_core::api::catalog::ProviderState::new(
            products,
            meters,
            is_sandbox,
            payment_api_url,
            networks_config,
            catalog_path,
            catalog_name,
            catalog_description,
            facilitator_pubkey,
            validator_rpc_urls,
            None, // kora_config
            None, // signer_pool
            ctx.manifest_path.clone(),
        );

        // Create IAC router for manifest management endpoints
        let manifest_file = ctx.manifest_path.join("moneymq.yaml");
        let iac_state = crate::iac::IacState::new(manifest_file);
        let iac_router = crate::iac::create_router(iac_state);

        // Start the combined server with both catalog and payment APIs
        moneymq_core::api::start_server(catalog_state, payment_api_state, Some(iac_router), port)
            .await
            .map_err(RunCommandError::ProviderStartError)?;

        Ok(())
    }
}
