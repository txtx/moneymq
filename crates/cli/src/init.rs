use std::{fs, path::Path};

use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

use crate::Context;

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct InitCommand {}

impl InitCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        println!("🚀 MoneyMQ Initialization\n");
        println!("This wizard will help you set up MoneyMQ with your payment provider.");
        println!("We'll create restricted API keys with the minimum required permissions.\n");

        // Step 1: Select provider
        let providers = vec!["Stripe"];
        let provider_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select your payment provider")
            .items(&providers)
            .default(0)
            .interact()
            .map_err(|e| format!("Failed to get provider selection: {}", e))?;

        let provider = providers[provider_selection].to_lowercase();
        println!("✓ Selected provider: {}\n", provider);

        // Step 2: Choose key type
        println!("\n🔑 API Key Setup");
        println!("Choose your setup method:");
        println!("  1. I have a restricted key (rk_live_...) - Recommended, limited permissions");
        println!("  2. Use my master secret key (sk_live_...) - Simpler, full access\n");

        let key_type_options = vec![
            "I have a restricted key already (recommended)",
            "Use my master secret key",
        ];
        let key_type_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select your setup method")
            .items(&key_type_options)
            .default(0)
            .interact()
            .map_err(|e| format!("Failed to get key type selection: {}", e))?;

        let use_restricted = key_type_selection == 0;

        // Step 3: Get keys based on selection
        let production_key = if use_restricted {
            println!("\n📱 Production Restricted Key Setup:");
            println!("\n✨ Required Permissions:");
            println!("  • Products: Write");
            println!("  • Prices: Write");
            println!("\n📋 Steps to create in Dashboard:");
            println!("  1. Open: https://dashboard.stripe.com/apikeys");
            println!("  2. Click '+ Create restricted key'");
            println!("  3. Name: 'MoneyMQ Production'");
            println!("  4. Enable: Products (Write) and Prices (Write)");
            println!("  5. Click 'Create key' and copy it\n");

            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Paste your restricted key (rk_live_...)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.starts_with("rk_live_") {
                        Ok(())
                    } else {
                        Err("Restricted key must start with rk_live_")
                    }
                })
                .interact_text()
                .map_err(|e| format!("Failed to get restricted key: {}", e))?
        } else {
            println!("\n📱 Production Master Key:");
            println!("⚠️  Warning: Master keys have full access to your Stripe account.");
            println!("   Consider using restricted keys for better security.\n");
            println!("Find your master key at: https://dashboard.stripe.com/apikeys\n");

            Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your master secret key (sk_live_...)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.starts_with("sk_live_") {
                        Ok(())
                    } else {
                        Err("Master key must start with sk_live_")
                    }
                })
                .interact_text()
                .map_err(|e| format!("Failed to get master key: {}", e))?
        };

        println!("✓ Production key received\n");

        // Step 4: Ask about sandbox
        let setup_sandbox = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Do you also want to set up a sandbox (test) environment?")
            .default(true)
            .interact()
            .map_err(|e| format!("Failed to get sandbox confirmation: {}", e))?;

        let sandbox_key = if setup_sandbox {
            if use_restricted {
                println!("\n📱 Sandbox Restricted Key Setup:");
                println!("\n✨ Required Permissions:");
                println!("  • Products: Write");
                println!("  • Prices: Write");
                println!("\n📋 Steps to create in Dashboard:");
                println!("  1. Switch to Test mode (toggle in top left)");
                println!("  2. Open: https://dashboard.stripe.com/test/apikeys");
                println!("  3. Click '+ Create restricted key'");
                println!("  4. Name: 'MoneyMQ Sandbox'");
                println!("  5. Enable: Products (Write) and Prices (Write)");
                println!("  6. Click 'Create key' and copy it\n");

                let key: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Paste your restricted key (rk_test_...)")
                    .validate_with(|input: &String| -> Result<(), &str> {
                        if input.starts_with("rk_test_") {
                            Ok(())
                        } else {
                            Err("Restricted key must start with rk_test_")
                        }
                    })
                    .interact_text()
                    .map_err(|e| format!("Failed to get sandbox restricted key: {}", e))?;

                println!("✓ Sandbox key received\n");
                Some(key)
            } else {
                println!("\n📱 Sandbox Master Key:");
                println!(
                    "Find your test master key at: https://dashboard.stripe.com/test/apikeys\n"
                );

                let key: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter your sandbox master secret key (sk_test_...)")
                    .validate_with(|input: &String| -> Result<(), &str> {
                        if input.starts_with("sk_test_") {
                            Ok(())
                        } else {
                            Err("Master key must start with sk_test_")
                        }
                    })
                    .interact_text()
                    .map_err(|e| format!("Failed to get sandbox master key: {}", e))?;

                println!("✓ Sandbox key received\n");
                Some(key)
            }
        } else {
            None
        };

        // Step 6: Create keys using the provider driver
        match provider.as_str() {
            "stripe" => {
                // Step 6: Save to .env file
                let env_path = ctx.manifest_path.join(".env");
                save_env_file(
                    &env_path,
                    &provider,
                    &production_key,
                    sandbox_key.as_deref(),
                )?;

                // Step 7: Generate Money.toml if it doesn't exist
                let manifest_toml_path = ctx.manifest_path.join("Money.toml");
                if !manifest_toml_path.exists() {
                    println!("Creating Money.toml...");
                    save_manifest_file(&manifest_toml_path, &provider, sandbox_key.is_some())?;
                    println!("✓ Created Money.toml\n");
                }

                // Step 8: Create catalog directory
                let catalog_path = ctx.manifest_path.join("catalog");
                if !catalog_path.exists() {
                    fs::create_dir(&catalog_path)
                        .map_err(|e| format!("Failed to create catalog directory: {}", e))?;
                    println!("✓ Created catalog/ directory\n");
                }

                // Step 9: Perform initial sync
                println!("Performing initial catalog sync...");
                // Reload environment to pick up new keys
                let _ = dotenvy::from_path(&env_path);

                // Use the new production key to fetch the catalog
                let mut catalog =
                    moneymq_driver_stripe::download_catalog(&production_key, &provider, true)
                        .await
                        .map_err(|e| format!("Failed to fetch catalog: {}", e))?;

                println!("✓ Downloaded {} products\n", catalog.total_count);

                // If sandbox key is provided, fetch sandbox catalog and match products
                if let Some(ref sandbox_key_value) = sandbox_key {
                    println!("Fetching sandbox catalog to match products...");

                    match moneymq_driver_stripe::download_catalog(
                        sandbox_key_value,
                        &provider,
                        false,
                    )
                    .await
                    {
                        Ok(sandbox_catalog) => {
                            println!(
                                "✓ Downloaded {} products from sandbox",
                                sandbox_catalog.total_count
                            );

                            // Match sandbox products to production products by name
                            let mut matched_count = 0;
                            for prod_product in &mut catalog.products {
                                // Find matching sandbox product by name
                                if let Some(sandbox_product) = sandbox_catalog
                                    .products
                                    .iter()
                                    .find(|sp| sp.name == prod_product.name && sp.name.is_some())
                                {
                                    if let Some(sandbox_id) = &sandbox_product.external_id {
                                        prod_product.sandbox_external_id = Some(sandbox_id.clone());
                                        matched_count += 1;
                                    }
                                }
                            }

                            if matched_count > 0 {
                                println!(
                                    "✓ Matched {} products with sandbox equivalents\n",
                                    matched_count
                                );
                            } else {
                                println!(
                                    "⚠️  No matching products found between production and sandbox\n"
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("⚠️  Warning: Failed to fetch sandbox catalog: {}", e);
                            eprintln!("    Continuing with production data only...\n");
                        }
                    }
                }

                // Save products as YAML files
                for product in &catalog.products {
                    let yaml_content = serde_yml::to_string(&product)
                        .map_err(|e| format!("Failed to serialize product: {}", e))?;

                    let product_path = catalog_path.join(format!("{}.yaml", product.id));
                    fs::write(&product_path, yaml_content)
                        .map_err(|e| format!("Failed to write product file: {}", e))?;
                }

                println!(
                    "✓ Saved {} product files to catalog/\n",
                    catalog.total_count
                );

                println!("✅ Initialization complete!");
                println!("\n🔐 Security Notes:");
                if use_restricted {
                    println!(
                        "  • Your restricted keys have minimal permissions (Products & Prices Write)"
                    );
                    println!("  • You can rotate these keys anytime through the Stripe Dashboard");
                } else {
                    println!("  • Your secret keys have full access to your Stripe account");
                    println!("  • Consider using restricted keys for better security");
                    println!("  • You can rotate these keys anytime through the Stripe Dashboard");
                }
                println!("\n📁 Created Files:");
                println!("  • {} - Environment configuration", env_path.display());
                println!("  • {} - Manifest file", manifest_toml_path.display());
                println!("  • catalog/ - {} product YAML files", catalog.total_count);
                println!("\n📚 Next Steps:");
                println!("  • Edit your product YAML files in catalog/");
                println!("  • Run 'moneymq catalog sync' to push changes to Stripe");
            }
            _ => return Err(format!("Unsupported provider: {}", provider)),
        }

        Ok(())
    }
}

fn save_env_file(
    path: &Path,
    provider: &str,
    production_key: &str,
    sandbox_key: Option<&str>,
) -> Result<(), String> {
    let mut content = String::new();

    content.push_str("# MoneyMQ Configuration\n");
    content.push_str("# Generated by 'moneymq init'\n\n");

    match provider {
        "stripe" => {
            content.push_str("# Stripe Production Key (restricted)\n");
            content.push_str(&format!("STRIPE_SECRET_KEY={}\n", production_key));

            if let Some(sandbox) = sandbox_key {
                content.push_str("\n# Stripe Sandbox/Test Key (restricted)\n");
                content.push_str(&format!("STRIPE_SANDBOX_SECRET_KEY={}\n", sandbox));
            }
        }
        _ => return Err(format!("Unsupported provider: {}", provider)),
    }

    fs::write(path, content).map_err(|e| format!("Failed to write .env file: {}", e))?;

    Ok(())
}

fn save_manifest_file(path: &Path, provider: &str, has_sandbox: bool) -> Result<(), String> {
    let sandbox_config = if has_sandbox {
        format!("\nsandbox = \"{}_sandbox\"", provider)
    } else {
        String::new()
    };

    let sandbox_provider = if has_sandbox {
        format!(
            r#"

[providers.{}_sandbox]
provider_type = "{}"
test_mode = true"#,
            provider, provider
        )
    } else {
        String::new()
    };

    let content = format!(
        r#"# MoneyMQ Manifest
# Generated by 'moneymq init'

[project]
name = "my-project"
version = "0.1.0"

[providers.{}]
provider_type = "{}"{}{}"#,
        provider, provider, sandbox_config, sandbox_provider
    );

    fs::write(path, content).map_err(|e| format!("Failed to write Money.toml: {}", e))?;

    Ok(())
}
