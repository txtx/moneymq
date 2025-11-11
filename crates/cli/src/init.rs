use std::{
    fs,
    path::{Path, PathBuf},
};

use console::style;
use dialoguer::{MultiSelect, Password, Select, theme::ColorfulTheme};
use indexmap::IndexMap;

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
            "Configure your billing as declarative code — with your existing products or a new setup.\n"
        );

        // Step 1: Choose how to begin
        let setup_options = vec!["Import from Stripe", "Create new catalog"];
        let setup_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose how to begin")
            .items(&setup_options)
            .default(0)
            .interact()
            .map_err(|e| format!("Failed to get setup selection: {}", e))?;

        let import_from_stripe = setup_selection == 0;

        // For now, we only support Stripe
        let provider = "stripe".to_string();

        // If creating new catalog, skip API key setup and just create directories
        if !import_from_stripe {
            let provider_name = "stripe".to_string();
            let catalog_version = "v1"; // Default version for new projects

            // Scaffold the project structure
            moneymq_mcp::scaffold::scaffold_moneymq_project(
                &ctx.manifest_path,
                &provider_name,
                catalog_version,
            )?;

            println!(
                "{} {} ./{}",
                style("✔").green(),
                style("Created").dim(),
                moneymq_types::MANIFEST_FILE_NAME
            );
            println!(
                "{} {} ./billing/{}/products",
                style("✔").green(),
                style("Created").dim(),
                catalog_version
            );
            println!(
                "{} {} ./billing/{}/meters",
                style("✔").green(),
                style("Created").dim(),
                catalog_version
            );
            println!(
                "{} {} ./billing/{}/assets",
                style("✔").green(),
                style("Created").dim(),
                catalog_version
            );

            // Offer to configure MCP server in editor
            println!();
            offer_mcp_configuration()?;

            // Show next steps after MCP configuration
            println!(
                "🎉 {}",
                style("Congratulations! Your billing infrastructure is setup.").bold()
            );
            println!();
            println!(
                "{} Open your code editor and describe the products you'd want to sell:",
                style("Next:").yellow().bold()
            );
            println!(
                "{}",
                style(
                    "  \"I'd like to sell a monthly subscription for $29/month with a 7-day trial\""
                )
                .dim()
            );

            return Ok(());
        }

        // Import from Stripe flow
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
                    moneymq_core::catalog::stripe::iac::get_account_info(&production_key)
                        .await
                        .map_err(|e| format!("Failed to fetch account info: {}", e))?;

                let sandbox_account_info = if let Some(ref sandbox_key_value) = sandbox_key {
                    let info =
                        moneymq_core::catalog::stripe::iac::get_account_info(sandbox_key_value)
                            .await
                            .ok();
                    info
                } else {
                    None
                };

                let provider_name = generate_provider_name(&account_info, "stripe");
                let catalog_version = generate_catalog_version(&account_info);

                // Save configuration
                let env_path = ctx.manifest_path.join(".env");
                save_env_file(
                    &env_path,
                    &provider,
                    &production_key,
                    sandbox_key.as_deref(),
                )?;

                let manifest_yaml_path = ctx.manifest_path.join(moneymq_types::MANIFEST_FILE_NAME);
                if !manifest_yaml_path.exists() {
                    // Create manifest with catalog configuration
                    let mut manifest = crate::manifest::Manifest {
                        catalogs: IndexMap::new(),
                        payments: IndexMap::new(), // Empty - will trigger footer in save()
                    };

                    // Build catalog config
                    let account_name = extract_account_name(&account_info);
                    let catalog_path = format!("billing/{}", catalog_version);

                    let mut catalog_config = crate::manifest::CatalogConfig {
                        description: Some(account_name.to_string()),
                        catalog_path,
                        source: Some(crate::manifest::CatalogSourceType::Stripe(
                            crate::manifest::StripeConfig {
                                api_key: None,
                                api_version: None,
                                webhook_endpoint: None,
                                webhook_secret_env: None,
                                sandboxes: IndexMap::new(),
                            },
                        )),
                    };

                    // Add sandbox if available
                    if sandbox_key.is_some() {
                        let sandbox_description = sandbox_account_info
                            .as_ref()
                            .map(|info| format!("Stripe sandbox - {}", extract_account_name(info)))
                            .unwrap_or_else(|| "Stripe sandbox".to_string());

                        let sandbox = crate::manifest::StripeSandboxConfig {
                            description: Some(sandbox_description),
                            api_key: None,
                            api_version: None,
                            webhook_endpoint: None,
                            webhook_secret_env: None,
                        };

                        if let Some(crate::manifest::CatalogSourceType::Stripe(ref mut stripe_config)) =
                            catalog_config.source
                        {
                            stripe_config.sandboxes.insert("default".to_string(), sandbox);
                        }
                    }

                    manifest.catalogs.insert(provider_name.clone(), catalog_config);

                    // Save using the new method (will add payments footer automatically)
                    manifest.save(&manifest_yaml_path)?;
                }

                // Create directories (silently, since we already have a manifest)
                let (catalog_path, meters_path, provider_assets_path) =
                    moneymq_mcp::scaffold::scaffold_moneymq_project(
                        &ctx.manifest_path,
                        &provider_name,
                        &catalog_version,
                    )?;

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

                let mut catalog = moneymq_core::catalog::stripe::iac::download_catalog(
                    &production_key,
                    &provider,
                    true,
                )
                .await
                .map_err(|e| format!("Failed to fetch catalog: {}", e))?;

                // If sandbox key is provided, fetch sandbox catalog and match products
                if let Some(ref sandbox_key_value) = sandbox_key {
                    match moneymq_core::catalog::stripe::iac::download_catalog(
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
                let mut meter_collection = moneymq_core::catalog::stripe::iac::download_meters(
                    &production_key,
                    &provider_name,
                    true,
                )
                .await
                .map_err(|e| format!("Failed to fetch meters: {}", e))?;

                // If sandbox key is provided, fetch sandbox meters and match
                if let Some(ref sandbox_key_value) = sandbox_key {
                    match moneymq_core::catalog::stripe::iac::download_meters(
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
                    let meter_path = meters_path.join(format!("{}.yaml", meter.id));
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

                // Offer to configure MCP server in editor
                println!();
                offer_mcp_configuration()?;

                // Show next steps after MCP configuration
                println!();
                println!(
                    "🎉 {}",
                    style("Congratulations! Your products are imported.").bold()
                );
                println!();
                println!(
                    "{}\nEdit products in billing/ or ask your AI agent to update your billing:",
                    style("Next:").yellow().bold()
                );
                println!(
                    "{}",
                    style("  \"Increase all my products prices by 7%\"").dim()
                );
                println!(
                    "{}",
                    style("  \"Add a $99 annual plan to my Premium product\"").dim()
                );
                println!();
                println!("{}", style("Then sync using the command:"));
                println!("  {}", style("moneymq catalog sync").cyan().bold());
                println!();
                println!(
                    "{}",
                    style("Or run MoneyMQ Studio to perform payment simulations:")
                );
                println!("  {}", style("moneymq start").cyan().bold());
            }
            _ => return Err(format!("Unsupported provider: {}", provider)),
        }

        Ok(())
    }
}

#[derive(Debug)]
enum Editor {
    VSCode,
    Cursor,
    Zed,
    ClaudeCode,
}

impl Editor {
    fn name(&self) -> &str {
        match self {
            Editor::VSCode => "VS Code",
            Editor::Cursor => "Cursor",
            Editor::Zed => "Zed",
            Editor::ClaudeCode => "Claude Code",
        }
    }

    fn config_path(&self) -> Option<PathBuf> {
        match self {
            Editor::VSCode => {
                // VS Code settings location varies by OS
                #[cfg(target_os = "macos")]
                let path =
                    dirs::home_dir()?.join("Library/Application Support/Code/User/settings.json");

                #[cfg(target_os = "linux")]
                let path = dirs::home_dir()?.join(".config/Code/User/settings.json");

                #[cfg(target_os = "windows")]
                let path = dirs::home_dir()?.join("AppData/Roaming/Code/User/settings.json");

                Some(path)
            }
            Editor::Cursor => {
                // Cursor uses ~/.cursor/mcp.json
                dirs::home_dir().map(|p| p.join(".cursor/mcp.json"))
            }
            Editor::Zed => {
                // Zed config location varies by OS
                #[cfg(target_os = "macos")]
                let path = dirs::home_dir()?.join(".config/zed/settings.json");

                #[cfg(target_os = "linux")]
                let path = dirs::home_dir()?.join(".config/zed/settings.json");

                #[cfg(target_os = "windows")]
                let path = dirs::home_dir()?.join("AppData/Roaming/Zed/settings.json");

                Some(path)
            }
            Editor::ClaudeCode => {
                // Claude Code config location varies by OS
                #[cfg(target_os = "macos")]
                let path = dirs::config_dir()?.join("Claude/claude_desktop_config.json");

                #[cfg(target_os = "linux")]
                let path = dirs::config_dir()?.join("Claude/claude_desktop_config.json");

                #[cfg(target_os = "windows")]
                let path = dirs::config_dir()?.join("Claude/claude_desktop_config.json");

                Some(path)
            }
        }
    }

    fn is_installed(&self) -> bool {
        match self {
            Editor::ClaudeCode => {
                // Check if either desktop app or CLI is installed
                let desktop_installed = self
                    .config_path()
                    .map(|p| p.parent().map(|p| p.exists()).unwrap_or(false))
                    .unwrap_or(false);

                // For CLI, check if current project exists in ~/.claude.json or if file exists at all
                let cli_installed = dirs::home_dir()
                    .and_then(|home| {
                        let cli_path = home.join(".claude.json");
                        if !cli_path.exists() {
                            return Some(false);
                        }

                        // If .claude.json exists, consider CLI installed
                        Some(true)
                    })
                    .unwrap_or(false);

                desktop_installed || cli_installed
            }
            _ => self
                .config_path()
                .map(|p| p.parent().map(|p| p.exists()).unwrap_or(false))
                .unwrap_or(false),
        }
    }

    fn has_moneymq_configured(&self) -> bool {
        match self {
            Editor::ClaudeCode => {
                // For Claude Code, we only check CLI config for the current project
                // This way each project gets its own MCP configuration prompt
                std::env::current_dir()
                    .ok()
                    .and_then(|current_dir| {
                        dirs::home_dir()
                            .map(|home| home.join(".claude.json"))
                            .filter(|p| p.exists())
                            .and_then(|p| fs::read_to_string(p).ok())
                            .and_then(|content| {
                                serde_json::from_str::<serde_json::Value>(&content).ok()
                            })
                            .map(|config| {
                                let project_key = current_dir.display().to_string();
                                config
                                    .get("projects")
                                    .and_then(|projects| projects.get(&project_key))
                                    .and_then(|project| project.get("mcpServers"))
                                    .and_then(|servers| servers.get("moneymq"))
                                    .is_some()
                            })
                    })
                    .unwrap_or(false)
            }
            _ => {
                // For other editors, check the standard config path
                let config_path = match self.config_path() {
                    Some(path) => path,
                    None => return false,
                };

                if !config_path.exists() {
                    return false;
                }

                let content = match fs::read_to_string(&config_path) {
                    Ok(content) => content,
                    Err(_) => return false,
                };

                match self {
                    Editor::VSCode => {
                        // Check for github.copilot.chat.mcp.servers.moneymq
                        if let Ok(settings) = json5::from_str::<serde_json::Value>(&content) {
                            settings
                                .get("github.copilot.chat.mcp.servers")
                                .and_then(|servers| servers.get("moneymq"))
                                .is_some()
                        } else {
                            false
                        }
                    }
                    Editor::Cursor => {
                        // Check for mcpServers.moneymq
                        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                            config
                                .get("mcpServers")
                                .and_then(|servers| servers.get("moneymq"))
                                .is_some()
                        } else {
                            false
                        }
                    }
                    Editor::Zed => {
                        // Check for context_servers.moneymq
                        if let Ok(settings) = json5::from_str::<serde_json::Value>(&content) {
                            settings
                                .get("context_servers")
                                .and_then(|servers| servers.get("moneymq"))
                                .is_some()
                        } else {
                            false
                        }
                    }
                    Editor::ClaudeCode => unreachable!(),
                }
            }
        }
    }
}

fn offer_mcp_configuration() -> Result<(), String> {
    // Detect installed editors that don't have moneymq configured yet
    let all_editors = vec![
        Editor::VSCode,
        Editor::Cursor,
        Editor::Zed,
        Editor::ClaudeCode,
    ];

    let installed_editors: Vec<Editor> = all_editors
        .into_iter()
        .filter(|e| e.is_installed() && !e.has_moneymq_configured())
        .collect();

    if installed_editors.is_empty() {
        return Ok(());
    }

    println!("{}", style("MoneyMQ MCP Server").bold());
    println!(
        "{}",
        style("Add MoneyMQ to your editor for AI-assisted catalog generation").dim()
    );
    println!();

    // Create display names for the editors
    let editor_names: Vec<String> = installed_editors
        .iter()
        .map(|e| e.name().to_string())
        .collect();

    // Use multi-select to let user choose which editors to configure
    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select editors to configure (Space to select, Enter to confirm)")
        .items(&editor_names)
        .defaults(&vec![true; installed_editors.len()]) // All selected by default
        .interact()
        .map_err(|e| format!("Failed to get editor selection: {}", e))?;

    // Configure selected editors
    for idx in selections {
        let editor = &installed_editors[idx];
        match configure_mcp_for_editor(editor) {
            Ok(_) => {
                println!(
                    "{} MoneyMQ MCP configured for {}",
                    style("✔").green(),
                    editor.name()
                );
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to configure {}: {}",
                    style("⚠").yellow(),
                    editor.name(),
                    e
                );
            }
        }
    }
    println!();

    Ok(())
}

fn configure_mcp_for_editor(editor: &Editor) -> Result<(), String> {
    let config_path = editor
        .config_path()
        .ok_or_else(|| "Could not determine config path".to_string())?;

    match editor {
        Editor::VSCode => configure_vscode_mcp(&config_path),
        Editor::Cursor => configure_cursor_mcp(&config_path),
        Editor::Zed => configure_zed_mcp(&config_path),
        Editor::ClaudeCode => configure_claude_code_mcp(&config_path),
    }
}

fn configure_vscode_mcp(config_path: &Path) -> Result<(), String> {
    // VS Code uses settings.json for global MCP config
    let mut settings: serde_json::Value = if config_path.exists() {
        let content = fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;

        // Handle comments in JSON (VS Code settings can have comments)
        json5::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Add MCP server configuration
    if settings.get("github.copilot.chat.mcp.enabled").is_none() {
        settings["github.copilot.chat.mcp.enabled"] = serde_json::json!(true);
    }

    // Initialize github.copilot.chat.mcp.servers if it doesn't exist
    if settings.get("github.copilot.chat.mcp.servers").is_none() {
        settings["github.copilot.chat.mcp.servers"] = serde_json::json!({});
    }

    let mcp_servers = settings
        .get_mut("github.copilot.chat.mcp.servers")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get github.copilot.chat.mcp.servers object")?;

    // Get the full path to moneymq binary
    let moneymq_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("moneymq")))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cargo/bin/moneymq")))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "moneymq".to_string());

    // Add moneymq server
    mcp_servers.insert(
        "moneymq".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": moneymq_path,
            "args": ["mcp"]
        }),
    );

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    // Write back settings
    let json_str = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(config_path, json_str).map_err(|e| format!("Failed to write settings: {}", e))?;

    Ok(())
}

fn configure_cursor_mcp(config_path: &Path) -> Result<(), String> {
    // Cursor uses ~/.cursor/mcp.json
    let mut config: serde_json::Value = if config_path.exists() {
        let content =
            fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Initialize mcpServers if it doesn't exist
    if !config.get("mcpServers").is_some() {
        config["mcpServers"] = serde_json::json!({});
    }

    let mcp_servers = config
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get mcpServers object")?;

    // Get the full path to moneymq binary
    let moneymq_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("moneymq")))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cargo/bin/moneymq")))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "moneymq".to_string());

    // Add moneymq server
    mcp_servers.insert(
        "moneymq".to_string(),
        serde_json::json!({
            "command": moneymq_path,
            "args": ["mcp"],
            "env": {}
        }),
    );

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    // Write config
    let json_str = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(config_path, json_str).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

fn configure_zed_mcp(config_path: &Path) -> Result<(), String> {
    // Zed uses settings.json with context_servers
    let mut settings: serde_json::Value = if config_path.exists() {
        let content = fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;

        // Try to parse as JSON5 first (Zed allows comments)
        json5::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Initialize context_servers if it doesn't exist
    if !settings.get("context_servers").is_some() {
        settings["context_servers"] = serde_json::json!({});
    }

    let context_servers = settings
        .get_mut("context_servers")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get context_servers object")?;

    // Get the full path to moneymq binary
    let moneymq_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("moneymq")))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cargo/bin/moneymq")))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "moneymq".to_string());

    // Add moneymq server
    context_servers.insert(
        "moneymq".to_string(),
        serde_json::json!({
            "source": "custom",
            "command": moneymq_path,
            "args": ["mcp"],
            "env": {}
        }),
    );

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    // Write settings
    let json_str = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(config_path, json_str).map_err(|e| format!("Failed to write settings: {}", e))?;

    Ok(())
}

fn configure_claude_code_mcp(config_path: &Path) -> Result<(), String> {
    // Get the full path to moneymq binary
    let moneymq_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("moneymq")))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cargo/bin/moneymq")))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "moneymq".to_string());

    // Configure both desktop app and CLI configs
    // 1. Desktop app: ~/Library/Application Support/Claude/claude_desktop_config.json
    configure_claude_config_file(config_path, &moneymq_path)?;

    // 2. CLI: ~/.claude.json (project-specific configuration)
    if let Some(home) = dirs::home_dir() {
        let cli_config_path = home.join(".claude.json");
        if cli_config_path.exists() {
            // Get current working directory for project-specific config
            if let Ok(current_dir) = std::env::current_dir() {
                println!(
                    "Configuring Claude Code CLI for project: {}",
                    current_dir.display()
                );
                match configure_claude_cli_project(&cli_config_path, &current_dir, &moneymq_path) {
                    Ok(_) => println!("✓ Configured Claude Code CLI"),
                    Err(e) => eprintln!("Warning: Failed to configure Claude Code CLI: {}", e),
                }
            }
        } else {
            println!(
                "Claude Code CLI config not found at {}",
                cli_config_path.display()
            );
        }
    }

    Ok(())
}

fn configure_claude_cli_project(
    config_path: &Path,
    project_dir: &Path,
    moneymq_path: &str,
) -> Result<(), String> {
    // Read existing config
    let mut config: serde_json::Value = if config_path.exists() {
        let content =
            fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        return Ok(()); // Skip if file doesn't exist
    };

    // Initialize projects object if it doesn't exist
    if !config.get("projects").is_some() {
        config["projects"] = serde_json::json!({});
    }

    let projects = config
        .get_mut("projects")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get projects object")?;

    // Get or create project entry
    let project_key = project_dir.display().to_string();
    if !projects.contains_key(&project_key) {
        projects.insert(
            project_key.clone(),
            serde_json::json!({
                "allowedTools": [],
                "mcpContextUris": [],
                "mcpServers": {},
                "enabledMcpjsonServers": [],
                "disabledMcpjsonServers": [],
                "hasTrustDialogAccepted": false,
                "projectOnboardingSeenCount": 0,
                "hasClaudeMdExternalIncludesApproved": false,
                "hasClaudeMdExternalIncludesWarningShown": false
            }),
        );
    }

    let project = projects
        .get_mut(&project_key)
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get project object")?;

    // Initialize mcpServers if it doesn't exist
    if !project.get("mcpServers").is_some() {
        project.insert("mcpServers".to_string(), serde_json::json!({}));
    }

    let mcp_servers = project
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get mcpServers object")?;

    // Add moneymq server
    mcp_servers.insert(
        "moneymq".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": moneymq_path,
            "args": ["mcp"]
        }),
    );

    // Write config back
    let json_str = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(config_path, json_str).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

fn configure_claude_config_file(config_path: &Path, moneymq_path: &str) -> Result<(), String> {
    // Read or create config
    let mut config: serde_json::Value = if config_path.exists() {
        let content =
            fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Initialize mcpServers if it doesn't exist
    if !config.get("mcpServers").is_some() {
        config["mcpServers"] = serde_json::json!({});
    }

    let mcp_servers = config
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .ok_or("Failed to get mcpServers object")?;

    // Add moneymq server with full path
    mcp_servers.insert(
        "moneymq".to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": moneymq_path,
            "args": ["mcp"]
        }),
    );

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    // Write config
    let json_str = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(config_path, json_str).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Generate a catalog version slug from account info (kebab-case for paths)
/// Format: <company-slug> or "v1" if no business name
fn generate_catalog_version(
    account_info: &moneymq_core::catalog::stripe::iac::AccountInfo,
) -> String {
    // Try to use business name first, then display name
    let name = account_info
        .business_name
        .as_ref()
        .or(account_info.display_name.as_ref());

    if let Some(name) = name {
        // Convert to slug: lowercase, replace spaces/special chars with hyphens (kebab-case)
        let slug = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            // Remove consecutive hyphens
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        if !slug.is_empty() {
            return slug;
        }
    }

    // Fallback to v1
    "v1".to_string()
}

/// Generate a provider name from account info (snake_case for YAML keys)
/// Format: <company_slug> or "v1" if no business name
fn generate_provider_name(
    account_info: &moneymq_core::catalog::stripe::iac::AccountInfo,
    _provider_type: &str,
) -> String {
    let catalog_version = generate_catalog_version(account_info);

    // If we got a v1 fallback, just return it
    if catalog_version == "v1" {
        return catalog_version;
    }

    // Convert kebab-case to snake_case for valid YAML key
    catalog_version.replace('-', "_")
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

/// Helper to extract account name from account info
fn extract_account_name(account_info: &moneymq_core::catalog::stripe::iac::AccountInfo) -> &str {
    account_info
        .display_name
        .as_deref()
        .or(account_info.business_name.as_deref())
        .unwrap_or("(no name)")
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
    account_info: &moneymq_core::catalog::stripe::iac::AccountInfo,
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
