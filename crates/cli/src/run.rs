use std::fs;

use moneymq_core::{facilitator::FacilitatorConfig, validator};
use moneymq_types::Meter;
use moneymq_types::Product;
use x402_rs::{chain::NetworkProvider, network::SolanaNetwork};

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
        println!("🚀 Starting MoneyMQ Provider Server\n");

        // Load products from billing/catalog directory
        let billing_dir = ctx.manifest_path.join("billing");
        let catalog_dir = billing_dir.join("catalog");

        if !catalog_dir.exists() {
            return Err(format!(
                "Catalog directory not found: {}\nRun 'moneymq init' or 'moneymq catalog sync' first",
                catalog_dir.display()
            ));
        }

        println!("📂 Loading products from {}", catalog_dir.display());

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
                        eprintln!("⚠️  Warning: Failed to parse {}: {}", path.display(), e);
                        eprintln!("    Skipping this file.");
                    }
                }
            }
        }

        if products.is_empty() {
            return Err("No products found in catalog directory".to_string());
        }

        println!("✓ Loaded {} products", products.len());

        // Load meters from billing/metering directory
        let metering_dir = billing_dir.join("metering");
        let mut meters = Vec::new();

        if metering_dir.exists() {
            println!("📂 Loading meters from {}", metering_dir.display());

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
                            eprintln!("⚠️  Warning: Failed to parse {}: {}", path.display(), e);
                            eprintln!("    Skipping this file.");
                        }
                    }
                }
            }

            println!("✓ Loaded {} meters\n", meters.len());
        } else {
            println!("⚠️  No metering directory found, starting without meters\n");
        }

        let mode = if self.sandbox {
            "sandbox"
        } else {
            "production"
        };
        println!("🔧 Mode: {}", mode);
        println!("🌐 Port: {}\n", self.port);

        println!("📡 API Endpoints:");
        println!("  GET http://localhost:{}/v1/products", self.port);
        println!("  GET http://localhost:{}/v1/prices", self.port);
        println!("  GET http://localhost:{}/v1/billing/meters", self.port);
        println!("  GET http://localhost:{}/health\n", self.port);

        println!("💡 Tip: Use this as your Stripe API endpoint for local development");
        println!("   Set STRIPE_API_BASE=http://localhost:{}", self.port);
        println!();
        println!("Press Ctrl+C to stop the server\n");

        // Initialize tracing
        tracing_subscriber::fmt::init();

        let mut handles = None;
        // Only start local facilitator server in sandbox mode
        if self.sandbox {
            let mut sandbox_x402_config = ctx
                .manifest
                .providers
                .iter()
                .filter_map(|(name, provider)| {
                    provider.x402_config().and_then(|config| {
                        if config.test_mode && config.local_facilitator.is_some() {
                            Some((name.clone(), config.clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect::<Vec<_>>();

            if sandbox_x402_config.len() > 1 {
                println!(
                    "⚠️  Warning: Multiple X402 sandbox providers found in manifest. Only the first local facilitator ({}) will be started.",
                    sandbox_x402_config[0].0
                );
            }
            if !sandbox_x402_config.is_empty() {
                let (x402_name, x402_config) = sandbox_x402_config.remove(0);
                let local_facilitator_config: FacilitatorConfig = x402_config
                    .local_facilitator
                    .unwrap()
                    .try_into()
                    .map_err(|e| {
                        format!(
                            "Failed to parse local facilitator config for provider '{}': {}",
                            x402_name, e
                        )
                    })?;

                let mut validator_handles = vec![];
                if local_facilitator_config.provider_cache.is_none() {
                    println!(
                        "⚠️  Warning: No providers configured for local facilitator of provider '{}'. Facilitator will have not be started.",
                        x402_name
                    );
                } else {
                    for (_, network_provider) in local_facilitator_config
                        .provider_cache
                        .as_ref()
                        .unwrap()
                        .into_iter()
                    {
                        match network_provider {
                            NetworkProvider::Evm(_) => {}
                            NetworkProvider::Solana(solana_provider) => {
                                match solana_provider.solana_network() {
                                    SolanaNetwork::LocalSurfnet => {
                                        let validator_config =
                                            moneymq_core::validator::SolanaValidatorConfig::new(
                                                solana_provider.rpc_url(),
                                                solana_provider.facilitator_pubkey().to_string(),
                                            );
                                        if let Some(handle) = validator::start_local_solana_validator(&validator_config)
                                            .map_err(|e| {
                                                format!(
                                                    "Failed to start local Solana validator for x402 provider '{}': {}",
                                                    x402_name, e
                                                )
                                        })? {
                                            validator_handles.push(handle);
                                            println!(
                                                "🔧 Started local Solana validator for x402 provider '{}' on {}",
                                                x402_name,
                                                solana_provider.rpc_url()
                                            );
                                            println!(
                                                "Initializing facilitator account {} with funds...",
                                                solana_provider.facilitator_pubkey()
                                            );
                                        }
                                        else {
                                            println!(
                                                "Local Solana validator already running at {}",
                                                validator_config.rpc_api_url
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    println!(
                        "🔧 Starting local X402 Facilitator for provider '{}' on http://{}:{}",
                        x402_name, local_facilitator_config.host, local_facilitator_config.port
                    );
                    let handle = moneymq_core::facilitator::start_local_facilitator(
                        &local_facilitator_config,
                    )
                    .await
                    .map_err(|e| format!("Failed to start local facilitator: {}", e))?;
                    handles = Some((handle, validator_handles));
                }
            }
        }

        // Start the server
        moneymq_core::provider::start_provider(products, meters, self.port, self.sandbox)
            .await
            .map_err(|e| format!("Failed to start server: {}", e))?;

        Ok(())
    }
}
