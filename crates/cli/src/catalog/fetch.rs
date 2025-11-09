use std::env;

use clap::Parser;
use moneymq_core::provider::stripe::iac::download_catalog;

use crate::Context;

#[derive(Parser, PartialEq, Clone, Debug)]
pub struct FetchCommand {
    /// Stripe API secret key. If not provided, will check STRIPE_SECRET_KEY env var or manifest
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
        let provider_name = ctx.provider.clone();
        let provider_config = ctx.manifest.get_catalog(&provider_name).ok_or_else(|| {
            format!(
                "Catalog '{}' not found in manifest. Available catalogs: {}",
                provider_name,
                if ctx.manifest.catalogs.is_empty() {
                    "none".to_string()
                } else {
                    ctx.manifest
                        .catalogs
                        .keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )
        })?;

        // Verify it's a Stripe catalog
        let stripe_config = provider_config.stripe_config().ok_or_else(|| {
            format!(
                "Catalog '{}' is not a Stripe catalog, but this command requires a Stripe catalog",
                provider_name
            )
        })?;

        // Get API key based on sandbox mode
        let (api_key, _env_var_name) = if ctx.use_sandbox {
            // Look for the "default" sandbox
            if let Some(sandbox_config) = stripe_config.sandboxes.get("default") {
                eprintln!(
                    "ℹ️  Sandbox mode: using 'default' sandbox configuration for provider '{}'",
                    provider_name
                );

                // Get API key with priority:
                // 1. Command-line flag (--api-key)
                // 2. Environment variable (STRIPE_SANDBOX_SECRET_KEY)
                // 3. Manifest file (providers.<name>.sandboxes.default.api_key)
                let key = match &self.api_key {
                    Some(key) => key.clone(),
                    None => {
                        match env::var("STRIPE_SANDBOX_SECRET_KEY") {
                            Ok(key) => key,
                            Err(_) => {
                                sandbox_config.api_key()
                                    .ok_or_else(|| {
                                        format!(
                                            "Stripe sandbox API key not found for provider '{}'. Please provide --api-key, set STRIPE_SANDBOX_SECRET_KEY environment variable, or configure api_key in manifest",
                                            provider_name
                                        )
                                    })?
                                    .clone()
                            }
                        }
                    }
                };
                (key, "STRIPE_SANDBOX_SECRET_KEY")
            } else {
                return Err(format!(
                    "Sandbox mode requested but provider '{}' does not have a 'default' sandbox configuration. Add a 'sandboxes.default' section to the provider config in manifest",
                    ctx.provider
                ));
            }
        } else {
            // Production mode - use main config
            // Get API key with priority:
            // 1. Command-line flag (--api-key)
            // 2. Environment variable (STRIPE_SECRET_KEY)
            // 3. Manifest file (providers.<name>.api_key)
            let key = match &self.api_key {
                Some(key) => key.clone(),
                None => {
                    match env::var("STRIPE_SECRET_KEY") {
                        Ok(key) => key,
                        Err(_) => {
                            stripe_config.api_key
                                .as_ref()
                                .ok_or_else(|| {
                                    format!(
                                        "Stripe API key not found for provider '{}'. Please provide --api-key, set STRIPE_SECRET_KEY environment variable, or configure api_key in manifest",
                                        provider_name
                                    )
                                })?
                                .clone()
                        }
                    }
                }
            };
            (key, "STRIPE_SECRET_KEY")
        };

        println!(
            "Fetching catalog from Stripe (provider: {})...",
            provider_name
        );

        // Determine if this is production (not in sandbox mode)
        let is_production = !ctx.use_sandbox;

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
