use std::fs;

use console::style;
// TODO: Re-enable when refactoring X402 facilitator
// use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::Meter;
use moneymq_types::Product;

// use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};
use crate::Context;

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct RunCommand {
    /// Port to run the server on
    #[arg(long, default_value = "8488")]
    pub port: u16,

    /// Use sandbox mode (serve sandbox external IDs)
    #[arg(long)]
    pub sandbox: bool,
}

impl RunCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
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
            return Err(format!(
                "Catalog directory not found: {}\nRun 'moneymq init' or 'moneymq catalog sync' first",
                catalog_dir.display()
            ));
        }

        print!("{} ", style("Loading products").dim());

        let mut products = Vec::new();
        let entries = fs::read_dir(&catalog_dir)
            .map_err(|e| format!("Failed to read catalog directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

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
            return Err("No products found in catalog directory".to_string());
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
                .map_err(|e| format!("Failed to read metering directory: {}", e))?;

            for entry in meter_entries {
                let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                    let content = fs::read_to_string(&path)
                        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

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

        #[allow(unused_variables)]
        let handles: Option<()> = None;
        // Only start local facilitator server in sandbox mode
        if self.sandbox {
            let sandbox_x402_config = ctx
                .manifest
                .providers
                .iter()
                .filter_map(|(name, provider)| {
                    provider.x402_config().and_then(|config| {
                        // Check if there's a "default" sandbox configuration with local facilitator
                        config.sandboxes.get("default").and_then(|sandbox| {
                            if sandbox.local_facilitator.is_some() {
                                Some((name.clone(), sandbox.clone()))
                            } else {
                                None
                            }
                        })
                    })
                })
                .collect::<Vec<_>>();

            if sandbox_x402_config.len() > 1 {
                eprintln!(
                    "{} Multiple X402 sandbox providers found in manifest. Only the first local facilitator ({}) will be started.",
                    style("Warning:").yellow(),
                    sandbox_x402_config[0].0
                );
            }
            // TODO: Re-enable X402 facilitator after refactoring to match new FacilitatorConfig structure
            if !sandbox_x402_config.is_empty() {
                eprintln!(
                    "{} X402 local facilitator startup is temporarily disabled during refactoring",
                    style("Warning:").yellow()
                );
            }
        }

        // Start the server
        moneymq_core::provider::start_provider(products, meters, self.port, self.sandbox)
            .await
            .map_err(|e| format!("Failed to start server: {}", e))?;

        Ok(())
    }
}
