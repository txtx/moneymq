use std::{fs, path::Path};

use console::style;
use dialoguer::{Password, Select, theme::ColorfulTheme};

use crate::Context;

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct InitCommand {}

impl InitCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        // Print banner
        // println!();
        // println!(" ::::::::::::::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!(":::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!("::::::::::::::::::::::::::::");
        // println!(" ::::::::::::::::::::::::::");
        // println!();
        // println!("          {}{}",
        //     style("Money").white().bold(),
        //     style("MQ").cyan().bold()
        // );
        // println!();
        println!(
            "{}{} {}\n",
            style("Money"),
            style("MQ").green(),
            style("helps you manage your billing using infrastructure as code.").dim()
        );
        // Step 1: Select provider
        let providers = vec!["Stripe"];
        let provider_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select your payment provider")
            .items(&providers)
            .default(0)
            .interact()
            .map_err(|e| format!("Failed to get provider selection: {}", e))?;

        let provider = providers[provider_selection].to_lowercase();

        // Step 2: Choose key type
        let key_type_options = vec![
            "Restricted key (rk_live_...) - recommended",
            "Master secret key (sk_live_...)",
        ];
        let key_type_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("API key type")
            .items(&key_type_options)
            .default(0)
            .interact()
            .map_err(|e| format!("Failed to get key type selection: {}", e))?;

        let use_restricted = key_type_selection == 0;

        // Step 3: Get production key
        let production_key = if use_restricted {
            println!("\nCreate at: https://dashboard.stripe.com/apikeys");
            println!("Permissions needed: Products (Write), Prices (Write)\n");

            Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Production restricted key (rk_live_...)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.starts_with("rk_live_") {
                        Ok(())
                    } else {
                        Err("Must start with rk_live_")
                    }
                })
                .interact()
                .map_err(|e| format!("Failed to get restricted key: {}", e))?
        } else {
            Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Production secret key (sk_live_...)")
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.starts_with("sk_live_") {
                        Ok(())
                    } else {
                        Err("Must start with sk_live_")
                    }
                })
                .interact()
                .map_err(|e| format!("Failed to get master key: {}", e))?
        };

        // Step 4: Get sandbox key (optional - leave empty to skip)
        let expected_prefix = if use_restricted {
            "rk_test_"
        } else {
            "sk_test_"
        };

        let sandbox_key: Option<String> = Password::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Sandbox secret key ({}) [press Enter to skip]",
                expected_prefix
            ))
            .allow_empty_password(true)
            .validate_with(move |input: &String| -> Result<(), &str> {
                if input.is_empty() || input.starts_with(expected_prefix) {
                    Ok(())
                } else {
                    Err("Must start with rk_test_ or sk_test_")
                }
            })
            .interact()
            .map_err(|e| format!("Failed to get sandbox key: {}", e))
            .map(|s| if s.is_empty() { None } else { Some(s) })?;

        // Step 6: Create keys using the provider driver
        match provider.as_str() {
            "stripe" => {
                // Fetch account information to validate production key
                let account_info =
                    moneymq_core::provider::stripe::iac::get_account_info(&production_key)
                        .await
                        .map_err(|e| format!("Failed to fetch account info: {}", e))?;

                let sandbox_account_info = if let Some(ref sandbox_key_value) = sandbox_key {
                    let info =
                        moneymq_core::provider::stripe::iac::get_account_info(sandbox_key_value)
                            .await
                            .ok();
                    info
                } else {
                    None
                };

                let provider_name = generate_provider_name(&account_info, "stripe");

                // Save configuration
                let env_path = ctx.manifest_path.join(".env");
                save_env_file(
                    &env_path,
                    &provider,
                    &production_key,
                    sandbox_key.as_deref(),
                )?;

                let manifest_yaml_path = ctx.manifest_path.join("billing.yaml");
                if !manifest_yaml_path.exists() {
                    save_manifest_file(
                        &manifest_yaml_path,
                        &provider_name,
                        &account_info,
                        sandbox_account_info.as_ref(),
                        sandbox_key.is_some(),
                    )?;
                }

                // Create directories
                let catalog_path = ctx.manifest_path.join("billing/catalog/v1");
                let metering_path = ctx.manifest_path.join("billing/metering/v1");
                let provider_assets_path = ctx
                    .manifest_path
                    .join(format!("billing/assets/{}", provider_name));

                if !catalog_path.exists() {
                    fs::create_dir_all(&catalog_path)
                        .map_err(|e| format!("Failed to create catalog directory: {}", e))?;
                }
                if !metering_path.exists() {
                    fs::create_dir_all(&metering_path)
                        .map_err(|e| format!("Failed to create metering directory: {}", e))?;
                }
                if !provider_assets_path.exists() {
                    fs::create_dir_all(&provider_assets_path).map_err(|e| {
                        format!("Failed to create provider assets directory: {}", e)
                    })?;
                }

                // Download and save logo/icon
                let logo_url = account_info
                    .logo_url
                    .as_ref()
                    .or(account_info.image_url.as_ref());
                if let Some(url) = logo_url {
                    let logo_path = provider_assets_path.join("logo.png");
                    match download_image(url, &logo_path).await {
                        Ok(_) => {
                            println!(
                                "{} {} ./billing/assets/{}/logo.png",
                                style("✔").green(),
                                style("Logo saved to").dim(),
                                provider_name
                            );
                        }
                        Err(e) => {
                            eprintln!("{} Failed to download logo: {}", style("⚠").yellow(), e);
                        }
                    }
                }

                // Save style.json with colors
                if account_info.primary_color.is_some() || account_info.secondary_color.is_some() {
                    let style_path = provider_assets_path.join("style.json");
                    match save_style_json(&style_path, &account_info) {
                        Ok(_) => {
                            println!(
                                "{} {} ./billing/assets/{}/style.json",
                                style("✔").green(),
                                style("Style saved to").dim(),
                                provider_name
                            );
                        }
                        Err(e) => {
                            eprintln!("{} Failed to save style: {}", style("⚠").yellow(), e);
                        }
                    }
                }

                // Fetch catalog
                println!("\nFetching products and meters...");
                let _ = dotenvy::from_path(&env_path);

                let mut catalog = moneymq_core::provider::stripe::iac::download_catalog(
                    &production_key,
                    &provider,
                    true,
                )
                .await
                .map_err(|e| format!("Failed to fetch catalog: {}", e))?;

                // If sandbox key is provided, fetch sandbox catalog and match products
                if let Some(ref sandbox_key_value) = sandbox_key {
                    match moneymq_core::provider::stripe::iac::download_catalog(
                        sandbox_key_value,
                        &provider,
                        false,
                    )
                    .await
                    {
                        Ok(sandbox_catalog) => {
                            // Match sandbox products to production products by name
                            let mut matched_count = 0;
                            let mut matched_sandbox_ids = std::collections::HashSet::new();

                            for prod_product in &mut catalog.products {
                                // Find matching sandbox product by name
                                if let Some(sandbox_product) = sandbox_catalog
                                    .products
                                    .iter()
                                    .find(|sp| sp.name == prod_product.name && sp.name.is_some())
                                {
                                    // The sandbox product should have its ID in sandboxes["default"]
                                    if let Some(sandbox_id) =
                                        sandbox_product.sandboxes.get("default")
                                    {
                                        // Add the sandbox ID to the production product
                                        prod_product
                                            .sandboxes
                                            .insert("default".to_string(), sandbox_id.clone());
                                        matched_sandbox_ids.insert(sandbox_id.clone());
                                        matched_count += 1;

                                        // Also match prices within the product
                                        for prod_price in &mut prod_product.prices {
                                            if let Some(sandbox_price) = sandbox_product
                                                .prices
                                                .iter()
                                                .find(|sp| sp.nickname == prod_price.nickname)
                                            {
                                                if let Some(sandbox_price_id) =
                                                    sandbox_price.sandboxes.get("default")
                                                {
                                                    prod_price.sandboxes.insert(
                                                        "default".to_string(),
                                                        sandbox_price_id.clone(),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Add sandbox-only products (products that exist in sandbox but not in production)
                            let mut sandbox_only_count = 0;
                            for sandbox_product in &sandbox_catalog.products {
                                // Check if this sandbox product was matched to a production product
                                if let Some(sandbox_id) = sandbox_product.sandboxes.get("default") {
                                    if !matched_sandbox_ids.contains(sandbox_id) {
                                        // This is a sandbox-only product - add it to the catalog
                                        let mut new_product = sandbox_product.clone();
                                        // Clear deployed_id since it doesn't exist in production
                                        new_product.deployed_id = None;
                                        catalog.products.push(new_product);
                                        sandbox_only_count += 1;
                                    }
                                }
                            }

                            let total_changes = matched_count + sandbox_only_count;
                            if total_changes > 0 {
                                println!(
                                    "{} Matched {} sandbox items",
                                    style("✓").green(),
                                    total_changes
                                );
                            }
                        }
                        Err(_) => {}
                    }
                }

                // Update total count after adding sandbox-only products
                catalog.total_count = catalog.products.len();

                // Save products as YAML files
                for product in &catalog.products {
                    let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                        &product,
                        Some("Product"),
                        Some("v1"),
                    )?;

                    let product_path = catalog_path.join(format!("{}.yaml", product.id));
                    fs::write(&product_path, yaml_content)
                        .map_err(|e| format!("Failed to write product file: {}", e))?;

                    // Display relative path from manifest directory
                    let relative_path = product_path
                        .strip_prefix(&ctx.manifest_path)
                        .unwrap_or(&product_path);
                    println!(
                        "{} {} ./{}",
                        style("✔").green(),
                        style("Product saved to").dim(),
                        relative_path.display()
                    );
                }

                // Fetch and save meters
                let mut meter_collection = moneymq_core::provider::stripe::iac::download_meters(
                    &production_key,
                    &provider_name,
                    true,
                )
                .await
                .map_err(|e| format!("Failed to fetch meters: {}", e))?;

                // If sandbox key is provided, fetch sandbox meters and match
                if let Some(ref sandbox_key_value) = sandbox_key {
                    match moneymq_core::provider::stripe::iac::download_meters(
                        sandbox_key_value,
                        &format!("{}_sandbox", provider_name),
                        false,
                    )
                    .await
                    {
                        Ok(sandbox_meter_collection) => {
                            // Track which sandbox meters were matched
                            let mut matched_sandbox_ids = std::collections::HashSet::new();

                            // Match sandbox meters to production meters by event_name
                            let mut _matched_count = 0;
                            for prod_meter in &mut meter_collection.meters {
                                if let Some(sandbox_meter) = sandbox_meter_collection
                                    .meters
                                    .iter()
                                    .find(|sm| sm.event_name == prod_meter.event_name)
                                {
                                    if let Some(sandbox_id) = sandbox_meter.sandboxes.get("default")
                                    {
                                        prod_meter
                                            .sandboxes
                                            .insert("default".to_string(), sandbox_id.clone());
                                        matched_sandbox_ids.insert(sandbox_id.clone());
                                        _matched_count += 1;
                                    }
                                }
                            }

                            // Add sandbox-only meters
                            let mut _sandbox_only_count = 0;
                            for sandbox_meter in &sandbox_meter_collection.meters {
                                if let Some(sandbox_id) = sandbox_meter.sandboxes.get("default") {
                                    if !matched_sandbox_ids.contains(sandbox_id) {
                                        let mut new_meter = sandbox_meter.clone();
                                        new_meter.deployed_id = None;
                                        meter_collection.meters.push(new_meter);
                                        _sandbox_only_count += 1;
                                    }
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }

                // Update total count and save meters
                meter_collection.total_count = meter_collection.meters.len();

                for meter in &meter_collection.meters {
                    let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                        &meter,
                        Some("Meter"),
                        Some("v1"),
                    )?;
                    let meter_path = metering_path.join(format!("{}.yaml", meter.id));
                    fs::write(&meter_path, yaml_content)
                        .map_err(|e| format!("Failed to write meter file: {}", e))?;

                    // Display relative path from manifest directory
                    let relative_path = meter_path
                        .strip_prefix(&ctx.manifest_path)
                        .unwrap_or(&meter_path);
                    println!(
                        "{} {} ./{}",
                        style("✔").green(),
                        style("Meter saved to").dim(),
                        relative_path.display()
                    );
                }

                println!(
                    "\n{}: Edit YAML files in billing/ and run 'moneymq catalog sync'",
                    style("Next steps").yellow()
                );
            }
            _ => return Err(format!("Unsupported provider: {}", provider)),
        }

        Ok(())
    }
}

/// Generate a provider name from account info
/// Format: <company_slug>_stripe or just "stripe" if no business name
fn generate_provider_name(
    account_info: &moneymq_core::provider::stripe::iac::AccountInfo,
    provider_type: &str,
) -> String {
    // Try to use business name first, then display name
    let name = account_info
        .business_name
        .as_ref()
        .or(account_info.display_name.as_ref());

    if let Some(name) = name {
        // Convert to slug: lowercase, replace spaces/special chars with underscores
        let slug = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            // Remove consecutive underscores
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_");

        if !slug.is_empty() {
            return format!("{}_{}", slug, provider_type);
        }
    }

    // Fallback to just the provider type
    provider_type.to_string()
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

fn save_manifest_file(
    path: &Path,
    provider_name: &str,
    account_info: &moneymq_core::provider::stripe::iac::AccountInfo,
    sandbox_account_info: Option<&moneymq_core::provider::stripe::iac::AccountInfo>,
    has_sandbox: bool,
) -> Result<(), String> {
    // Generate description from account info
    let account_name = account_info
        .display_name
        .as_deref()
        .or(account_info.business_name.as_deref())
        .unwrap_or("(no name)");

    let description = format!("{}", account_name);

    let sandbox_section = if has_sandbox {
        let sandbox_description = if let Some(sandbox_info) = sandbox_account_info {
            let sandbox_name = sandbox_info
                .display_name
                .as_deref()
                .or(sandbox_info.business_name.as_deref())
                .unwrap_or("(no name)");
            format!("Stripe sandbox - {}", sandbox_name)
        } else {
            "Stripe sandbox".to_string()
        };

        format!(
            r#"
    # The "default" sandbox is used when --sandbox is specified
    sandboxes:
      default:
        description: "{}"
        # api_key: sk_test_...  # Optional - overridden by STRIPE_SANDBOX_SECRET_KEY env var
        # api_version: "2023-10-16"  # Optional - defaults to Stripe's latest"#,
            sandbox_description
        )
    } else {
        r#"
    # The "default" sandbox is used when --sandbox is specified
    # sandboxes:
    #   default:
    #     description: "Stripe sandbox"
    #     api_key: sk_test_...  # Optional - overridden by STRIPE_SANDBOX_SECRET_KEY env var
    #     api_version: "2023-10-16"  # Optional - defaults to Stripe's latest"#
            .to_string()
    };

    let content = format!(
        r#"---
# MoneyMQ Billing Configuration - API version v1
# Generated by 'moneymq init'
# This file defines your billing providers and their configurations

providers:
  {}:
    provider_type: stripe
    description: "{}"
    # api_key: sk_live_...  # Optional - overridden by STRIPE_SECRET_KEY env var
    # api_version: "2023-10-16"  # Optional - defaults to Stripe's latest
    # catalog_path: billing/catalog/v1  # Optional - catalog path (default: billing/catalog/v1)
{}"#,
        provider_name, description, sandbox_section
    );

    fs::write(path, content).map_err(|e| format!("Failed to write billing.yaml: {}", e))?;

    Ok(())
}

/// Download an image from a URL and save it to a file
async fn download_image(url: &str, path: &Path) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to download image: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download image: HTTP {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    fs::write(path, bytes).map_err(|e| format!("Failed to write image file: {}", e))?;

    Ok(())
}

/// Save style information (colors) to a JSON file
fn save_style_json(
    path: &Path,
    account_info: &moneymq_core::provider::stripe::iac::AccountInfo,
) -> Result<(), String> {
    let mut style = serde_json::Map::new();

    if let Some(ref primary_color) = account_info.primary_color {
        style.insert(
            "primary_color".to_string(),
            serde_json::Value::String(primary_color.clone()),
        );
    }

    if let Some(ref secondary_color) = account_info.secondary_color {
        style.insert(
            "secondary_color".to_string(),
            serde_json::Value::String(secondary_color.clone()),
        );
    }

    let json = serde_json::to_string_pretty(&style)
        .map_err(|e| format!("Failed to serialize style JSON: {}", e))?;

    fs::write(path, json).map_err(|e| format!("Failed to write style.json: {}", e))?;

    Ok(())
}
