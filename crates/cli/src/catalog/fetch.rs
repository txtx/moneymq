use std::env;

use clap::Parser;
use moneymq_driver_stripe::download_catalog;

use crate::Context;

#[derive(Parser, PartialEq, Clone, Debug)]
pub struct FetchCommand {
    /// Stripe API secret key. If not provided, will check STRIPE_SECRET_KEY env var or Money.toml
    #[arg(long = "api-key", short = 'k')]
    pub api_key: Option<String>,

    /// Output format: json or pretty (default: pretty)
    #[arg(long = "format", short = 'f', default_value = "pretty")]
    pub format: OutputFormat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Json,
    Pretty,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "pretty" => Ok(OutputFormat::Pretty),
            _ => Err(format!(
                "Invalid format: {}. Valid options are: json, pretty",
                s
            )),
        }
    }
}

impl FetchCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        // Get the selected provider configuration
        let mut provider_name = ctx.provider.clone();
        let provider_config = ctx.manifest.get_provider(&provider_name).ok_or_else(|| {
            format!(
                "Provider '{}' not found in Money.toml. Available providers: {}",
                provider_name,
                if ctx.manifest.providers.is_empty() {
                    "none".to_string()
                } else {
                    ctx.manifest
                        .providers
                        .keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )
        })?;

        // Verify it's a Stripe provider
        let mut stripe_config = provider_config.stripe_config().ok_or_else(|| {
            format!(
                "Provider '{}' is type {}, but this command requires a Stripe provider",
                provider_name, provider_config
            )
        })?;

        // Check if sandbox mode is requested
        if ctx.use_sandbox {
            if let Some(sandbox_provider_name) = &stripe_config.sandbox {
                eprintln!(
                    "ℹ️  Sandbox mode: switching from '{}' to '{}'",
                    provider_name, sandbox_provider_name
                );
                provider_name = sandbox_provider_name.clone();
                stripe_config = ctx.manifest.get_provider(&provider_name)
                    .ok_or_else(|| {
                        format!(
                            "Sandbox provider '{}' not found in Money.toml. Please check the sandbox configuration.",
                            provider_name
                        )
                    })?.stripe_config().ok_or_else(|| {
                        format!(
                            "Sandbox provider '{}' is type {}, but this command requires a Stripe provider",
                            provider_name, provider_config
                        )
                    })?;
            } else {
                return Err(format!(
                    "Sandbox mode requested but provider '{}' does not have a sandbox configuration. Add 'sandbox = \"provider_name\"' to the provider config.",
                    ctx.provider
                ));
            }
        }

        // Get API key with priority:
        // 1. Command-line flag (--api-key)
        // 2. Environment variable (STRIPE_SECRET_KEY)
        // 3. Manifest file (providers.<name>.api_key)
        let api_key = match &self.api_key {
            Some(key) => key.clone(),
            None => {
                // Try environment variable first
                match env::var("STRIPE_SECRET_KEY") {
                    Ok(key) => key,
                    Err(_) => {
                        // Try manifest file
                        stripe_config.api_key
                            .as_ref()
                            .ok_or_else(|| {
                                format!(
                                    "Stripe API key not found for provider '{}'. Please provide --api-key, set STRIPE_SECRET_KEY environment variable, or configure api_key in Money.toml",
                                    provider_name
                                )
                            })?
                            .clone()
                    }
                }
            }
        };

        println!(
            "Fetching catalog from Stripe (provider: {})...",
            provider_name
        );

        // Determine if this is production (not in sandbox mode and provider is not in test_mode)
        let is_production = !ctx.use_sandbox && !stripe_config.test_mode;

        // Download the catalog
        let catalog = download_catalog(&api_key, &provider_name, is_production)
            .await
            .map_err(|e| format!("Failed to download catalog: {}", e))?;

        // Display the results
        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&catalog)
                    .map_err(|e| format!("Failed to serialize catalog: {}", e))?;
                println!("{}", json);
            }
            OutputFormat::Pretty => {
                println!(
                    "\n✓ Successfully downloaded {} products\n",
                    catalog.total_count
                );

                if catalog.products.is_empty() {
                    println!("No products found in your Stripe catalog.");
                } else {
                    println!(
                        "{:<40} {:<15} {:<10} {}",
                        "Product ID", "Name", "Active", "Description"
                    );
                    println!("{}", "-".repeat(100));

                    for product in &catalog.products {
                        let name = product.name.as_deref().unwrap_or("N/A");
                        let active = if product.active { "✓" } else { "✗" };
                        let description = product.description.as_deref().unwrap_or("");

                        // Truncate long descriptions
                        let description = if description.len() > 35 {
                            format!("{}...", &description[..32])
                        } else {
                            description.to_string()
                        };

                        println!(
                            "{:<40} {:<15} {:<10} {}",
                            product.id.to_string(),
                            &name[..name.len().min(15)],
                            active,
                            description
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
