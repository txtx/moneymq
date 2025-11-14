use std::{fs, path::PathBuf};

use console::{StyledObject, style};
use indexmap::IndexMap;
use moneymq_core::billing::{NetworksConfig, NetworksConfigError};
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::{Meter, x402::config::facilitator::ValidatorsConfig};
use moneymq_types::{Product, x402::MoneyMqNetwork};
use url::Url;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::{
    Context,
    manifest::{Manifest, x402::PaymentConfig},
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
    #[error("Failed to start local facilitator networks: {0}")]
    StartFacilitatorNetworks(String),
    #[error("Failed configure billing settings: {0}")]
    NetworksConfigInitializationError(NetworksConfigError),
    #[error("Failed to fund local accounts: {0}")]
    FundLocalAccountsError(String),
    #[error("Failed to start provider server: {0}")]
    ProviderStartError(Box<dyn std::error::Error>),
    #[error("Missing required billing networks")]
    NoBillingNetworksConfigured,
}

type BillingNetworksMap = IndexMap<String, (MoneyMqNetwork, Option<String>, Vec<String>)>;

pub trait ServiceCommand {
    const SANDBOX: bool;
    fn get() -> StyledObject<&'static str> {
        style("  GET").yellow()
    }

    fn post() -> StyledObject<&'static str> {
        style(" POST").magenta()
    }

    fn port(&self) -> u16;

    fn billing_networks(
        &self,
        manifest: &Manifest,
    ) -> Result<IndexMap<String, (MoneyMqNetwork, Option<String>, Vec<String>)>, RunCommandError>;

    fn networks_config(
        &self,
        billing_networks: BillingNetworksMap,
    ) -> Result<NetworksConfig, RunCommandError>;

    async fn setup_facilitator(
        &self,
        payments: &IndexMap<String, PaymentConfig>,
        networks_config: &NetworksConfig,
    ) -> Result<(Url, Option<String>, ValidatorsConfig), RunCommandError>;

    async fn execute(&self, ctx: &Context) -> Result<(), RunCommandError> {
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

        println!(
            "# {}{}{}",
            style("Catalog & Billing API (schema: ").dim(),
            style("stripe").green(),
            style(")").dim()
        );
        println!(
            " {} http://localhost:{}/v1/products",
            Self::get(),
            self.port()
        );
        println!(
            " {} http://localhost:{}/v1/billing/meters",
            Self::post(),
            self.port()
        );
        println!(" {}", style(" ...").dim());

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let billing_networks = self.billing_networks(&ctx.manifest)?;

        let networks_config = self.networks_config(billing_networks)?;

        // Build facilitator config once for sandbox mode
        let (facilitator_url, facilitator_pubkey, validator_rpc_urls) = self
            .setup_facilitator(&ctx.manifest.payments, &networks_config)
            .await?;

        println!();
        println!(
            "{}{} {}: {} - {}",
            style("Money").white(),
            style("MQ").green(),
            style("Studio:").white(),
            style(format!("http://localhost:{}", self.port())).cyan(),
            style("Press Ctrl+C to stop").dim()
        );
        println!();

        // let Some((_facilitator_handle, local_validator_ctx, facilitator_url)) = handles else {
        //     panic!("Facilitator must be started in sandbox mode");
        // };

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

        // Start the server
        moneymq_core::catalog::start_provider(
            products,
            meters,
            facilitator_url,
            self.port(),
            Self::SANDBOX,
            networks_config,
            catalog_path,
            catalog_name,
            catalog_description,
            facilitator_pubkey,
            validator_rpc_urls,
            None, // kora_config
            None, // signer_pool
        )
        .await
        .map_err(RunCommandError::ProviderStartError)?;

        Ok(())
    }
}
