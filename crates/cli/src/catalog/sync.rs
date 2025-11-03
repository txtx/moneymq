use std::{collections::HashMap, env, fs};

use clap::Parser;
use console::style;
use dialoguer::Confirm;
use moneymq_driver_stripe::{download_catalog, update_product};
use moneymq_types::Product;

use crate::Context;

#[derive(Debug, PartialEq)]
enum SyncAction {
    /// Remote is newer, overwrite local
    Pull,
    /// Local has changes, update remote
    Push,
    /// No changes needed
    NoChange,
    /// New product from remote
    Create,
}

/// Compare local and remote products to determine sync action
fn determine_sync_action(local: Option<&Product>, remote: &Product) -> SyncAction {
    match local {
        None => SyncAction::Create,
        Some(local_product) => {
            // Compare timestamps
            match (local_product.updated_at, remote.updated_at) {
                (Some(local_updated), Some(remote_updated)) => {
                    if remote_updated > local_updated {
                        // Remote is newer
                        SyncAction::Pull
                    } else if local_updated == remote_updated {
                        // Same timestamp - check if content differs
                        if products_differ(local_product, remote) {
                            SyncAction::Push
                        } else {
                            SyncAction::NoChange
                        }
                    } else {
                        // Local is newer
                        SyncAction::Push
                    }
                }
                (None, Some(_)) => SyncAction::Pull,
                (Some(_), None) => SyncAction::Push,
                (None, None) => {
                    if products_differ(local_product, remote) {
                        SyncAction::Push
                    } else {
                        SyncAction::NoChange
                    }
                }
            }
        }
    }
}

/// Check if two products have different content (excluding timestamps)
fn products_differ(local: &Product, remote: &Product) -> bool {
    use moneymq_types::normalize_metadata_for_comparison;

    // Normalize metadata for comparison to handle JSON formatting differences
    let local_metadata_normalized = normalize_metadata_for_comparison(&local.metadata);
    let remote_metadata_normalized = normalize_metadata_for_comparison(&remote.metadata);

    local.name != remote.name
        || local.description != remote.description
        || local.active != remote.active
        || local_metadata_normalized != remote_metadata_normalized
        || local.product_type != remote.product_type
        || local.images != remote.images
        || local.statement_descriptor != remote.statement_descriptor
        || local.unit_label != remote.unit_label
}

/// Display differences between local and remote products with colored diff output
fn display_diff(local: &Product, remote: &Product) {
    println!(
        "  {} {}",
        style("Product:").bold(),
        style(local.name.as_deref().unwrap_or("Unnamed"))
            .cyan()
            .bold()
    );
    println!("  {} {}", style("ID:").bold(), style(&local.id).dim());
    println!();

    if local.name != remote.name {
        println!("  {} {}", style("📝").yellow(), style("Name:").bold());
        println!(
            "    {} {}",
            style("-").red().bold(),
            style(remote.name.as_deref().unwrap_or("(none)")).red()
        );
        println!(
            "    {} {}",
            style("+").green().bold(),
            style(local.name.as_deref().unwrap_or("(none)")).green()
        );
        println!();
    }

    if local.description != remote.description {
        println!(
            "  {} {}",
            style("📝").yellow(),
            style("Description:").bold()
        );

        let remote_desc = remote.description.as_deref().unwrap_or("(none)");
        let local_desc = local.description.as_deref().unwrap_or("(none)");

        // Truncate long descriptions for readability
        let max_len = 80;
        let remote_display = if remote_desc.len() > max_len {
            format!("{}...", &remote_desc[..max_len])
        } else {
            remote_desc.to_string()
        };
        let local_display = if local_desc.len() > max_len {
            format!("{}...", &local_desc[..max_len])
        } else {
            local_desc.to_string()
        };

        println!(
            "    {} {}",
            style("-").red().bold(),
            style(remote_display).red()
        );
        println!(
            "    {} {}",
            style("+").green().bold(),
            style(local_display).green()
        );
        println!();
    }

    if local.active != remote.active {
        println!("  {} {}", style("📝").yellow(), style("Active:").bold());
        println!(
            "    {} {}",
            style("-").red().bold(),
            style(remote.active).red()
        );
        println!(
            "    {} {}",
            style("+").green().bold(),
            style(local.active).green()
        );
        println!();
    }

    if local.product_type != remote.product_type {
        println!(
            "  {} {}",
            style("📝").yellow(),
            style("Product Type:").bold()
        );
        println!(
            "    {} {}",
            style("-").red().bold(),
            style(remote.product_type.as_deref().unwrap_or("(none)")).red()
        );
        println!(
            "    {} {}",
            style("+").green().bold(),
            style(local.product_type.as_deref().unwrap_or("(none)")).green()
        );
        println!();
    }

    if local.metadata != remote.metadata {
        use std::collections::HashSet;

        use moneymq_types::normalize_metadata_for_comparison;

        let local_normalized = normalize_metadata_for_comparison(&local.metadata);
        let remote_normalized = normalize_metadata_for_comparison(&remote.metadata);

        // Find all keys that are different
        let all_keys: HashSet<_> = local_normalized
            .keys()
            .chain(remote_normalized.keys())
            .collect();

        let mut changes = Vec::new();
        for key in all_keys {
            let local_val = local_normalized.get(key.as_str());
            let remote_val = remote_normalized.get(key.as_str());

            if local_val != remote_val {
                changes.push((key.as_str(), local_val, remote_val));
            }
        }

        println!("  {} {}", style("📝").yellow(), style("Metadata:").bold());

        for (key, local_val, remote_val) in changes {
            println!();
            println!("    {}", style(key).cyan());

            // Convert both to YAML for comparison
            let remote_yaml = remote_val.and_then(|s| {
                serde_json::from_str::<serde_json::Value>(s)
                    .ok()
                    .and_then(|v| serde_yml::to_string(&v).ok())
            });

            let local_yaml = local_val.and_then(|s| {
                serde_json::from_str::<serde_json::Value>(s)
                    .ok()
                    .and_then(|v| serde_yml::to_string(&v).ok())
            });

            match (remote_yaml, local_yaml) {
                (Some(remote), Some(local)) => {
                    // Perform line-by-line diff
                    let remote_lines: Vec<&str> = remote.trim_end().lines().collect();
                    let local_lines: Vec<&str> = local.trim_end().lines().collect();

                    // Simple line-by-line comparison with context
                    let mut diff_lines = Vec::new();
                    let max_len = remote_lines.len().max(local_lines.len());

                    for i in 0..max_len {
                        let remote_line = remote_lines.get(i);
                        let local_line = local_lines.get(i);

                        match (remote_line, local_line) {
                            (Some(r), Some(l)) if r == l => {
                                // Lines match - show as context (only if near a change)
                                diff_lines.push((i, "context", *r));
                            }
                            (Some(r), Some(l)) => {
                                // Lines differ
                                diff_lines.push((i, "removed", *r));
                                diff_lines.push((i, "added", *l));
                            }
                            (Some(r), None) => {
                                // Line removed
                                diff_lines.push((i, "removed", *r));
                            }
                            (None, Some(l)) => {
                                // Line added
                                diff_lines.push((i, "added", *l));
                            }
                            (None, None) => {}
                        }
                    }

                    // Only show changed lines and 2 lines of context
                    let mut lines_to_show = Vec::new();
                    for i in 0..diff_lines.len() {
                        let (_, change_type, _) = &diff_lines[i];
                        if *change_type != "context" {
                            // Include this line and context
                            for j in i.saturating_sub(2)..=(i + 2).min(diff_lines.len() - 1) {
                                if !lines_to_show.contains(&j) {
                                    lines_to_show.push(j);
                                }
                            }
                        }
                    }
                    lines_to_show.sort();

                    // Display lines
                    let mut shown = 0;
                    for &idx in &lines_to_show {
                        if shown >= 15 {
                            // Limit total output
                            println!("      {}", style("...").dim());
                            break;
                        }

                        let (_, change_type, line) = &diff_lines[idx];
                        match *change_type {
                            "removed" => {
                                println!("      {} {}", style("-").red().bold(), style(line).red())
                            }
                            "added" => println!(
                                "      {} {}",
                                style("+").green().bold(),
                                style(line).green()
                            ),
                            "context" => {
                                println!("      {} {}", style(" ").dim(), style(line).dim())
                            }
                            _ => {}
                        }
                        shown += 1;
                    }
                }
                (Some(remote), None) => {
                    // Only remote exists
                    for line in remote.trim_end().lines().take(10) {
                        println!("      {} {}", style("-").red().bold(), style(line).red());
                    }
                }
                (None, Some(local)) => {
                    // Only local exists
                    for line in local.trim_end().lines().take(10) {
                        println!(
                            "      {} {}",
                            style("+").green().bold(),
                            style(line).green()
                        );
                    }
                }
                (None, None) => {
                    // Fallback to raw strings
                    if let Some(r) = remote_val {
                        println!("      {} {}", style("-").red().bold(), style(r).red());
                    }
                    if let Some(l) = local_val {
                        println!("      {} {}", style("+").green().bold(), style(l).green());
                    }
                }
            }
        }

        println!();
    }
}

#[derive(Parser, PartialEq, Clone, Debug)]
pub struct SyncCommand {
    /// Stripe API secret key. If not provided, will check STRIPE_SECRET_KEY env var or billing.yaml
    #[arg(long = "api-key", short = 'k')]
    pub api_key: Option<String>,
}

impl SyncCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        // Always fetch from production provider (ignore --sandbox flag for sync)
        let provider_name = ctx.provider.clone();
        let provider_config = ctx.manifest.get_provider(&provider_name).ok_or_else(|| {
            format!(
                "Provider '{}' not found in billing.yaml. Available providers: {}",
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

        // Verify it's a Stripe provider and get the catalog path
        let stripe_config = provider_config.stripe_config().ok_or_else(|| {
            format!(
                "Provider '{}' is type {}, but this command requires a Stripe provider",
                provider_name, provider_config
            )
        })?;

        let catalog_path = &stripe_config.catalog_path;

        // Get the catalog and metering directories
        let catalog_dir = ctx.manifest_path.join(catalog_path);
        let metering_path = catalog_path.replace("/catalog/", "/metering/");
        let metering_dir = ctx.manifest_path.join(&metering_path);

        // Load existing products from YAML files
        let mut local_products: HashMap<String, Product> = HashMap::new();
        if catalog_dir.exists() {
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
                            local_products.insert(product.id.clone(), product);
                        }
                        Err(e) => {
                            eprintln!("⚠️  Warning: Failed to parse {}: {}", path.display(), e);
                            eprintln!("    Skipping this file. You may need to regenerate it.");
                            continue;
                        }
                    }
                }
            }

            if !local_products.is_empty() {
                println!(
                    "✓ Loaded {} existing products from disk",
                    local_products.len()
                );
            }
        } else {
            // Create the catalog directory if it doesn't exist
            fs::create_dir_all(&catalog_dir)
                .map_err(|e| format!("Failed to create catalog directory: {}", e))?;
            println!("✓ Created catalog directory: {}", catalog_dir.display());
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
                                    "Stripe API key not found for provider '{}'. Please provide --api-key, set STRIPE_SECRET_KEY environment variable, or configure api_key in billing.yaml",
                                    provider_name
                                )
                            })?
                            .clone()
                    }
                }
            }
        };

        println!(
            "Fetching production catalog from Stripe (provider: {})...",
            provider_name
        );

        // Always sync from production (is_production = true)
        let is_production = !stripe_config.test_mode;

        // Download the production catalog
        let mut catalog = download_catalog(&api_key, &provider_name, is_production)
            .await
            .map_err(|e| format!("Failed to download catalog: {}", e))?;

        println!("✓ Downloaded {} products from remote", catalog.total_count);

        // Check if we have a "default" sandbox configuration in the manifest
        if let Some(_sandbox_config) = stripe_config.sandboxes.get("default") {
            // Try to get sandbox API key from environment
            let sandbox_api_key = env::var("STRIPE_SANDBOX_SECRET_KEY").ok();

            if let Some(sandbox_key) = sandbox_api_key {
                println!("Fetching sandbox catalog to match products...");

                match download_catalog(&sandbox_key, &format!("{}_sandbox", provider_name), false)
                    .await
                {
                    Ok(sandbox_catalog) => {
                        println!(
                            "✓ Downloaded {} products from sandbox",
                            sandbox_catalog.total_count
                        );

                        // Track which sandbox products were matched
                        let mut matched_sandbox_ids = std::collections::HashSet::new();

                        // Match sandbox products to production products by name
                        let mut matched_count = 0;
                        for prod_product in &mut catalog.products {
                            // Find matching sandbox product by name
                            if let Some(sandbox_product) = sandbox_catalog
                                .products
                                .iter()
                                .find(|sp| sp.name == prod_product.name && sp.name.is_some())
                            {
                                // Sandbox product has ID in sandboxes["default"]
                                if let Some(sandbox_id) = sandbox_product.sandboxes.get("default") {
                                    prod_product
                                        .sandboxes
                                        .insert("default".to_string(), sandbox_id.clone());
                                    matched_count += 1;
                                    matched_sandbox_ids.insert(sandbox_id.clone());
                                }
                            }
                        }

                        if matched_count > 0 {
                            println!(
                                "✓ Matched {} products with sandbox equivalents",
                                matched_count
                            );
                        }

                        // Find sandbox-only products (not in production)
                        let sandbox_only: Vec<_> = sandbox_catalog
                            .products
                            .into_iter()
                            .filter(|sp| {
                                sp.sandboxes
                                    .get("default")
                                    .map(|id| !matched_sandbox_ids.contains(id))
                                    .unwrap_or(false)
                            })
                            .collect();

                        if !sandbox_only.is_empty() {
                            println!(
                                "✓ Found {} sandbox-only products (not in production)",
                                sandbox_only.len()
                            );

                            // Save sandbox-only products to disk - they already have sandboxes["default"] set
                            for sandbox_product in sandbox_only {
                                // Sandbox products already have their ID in sandboxes["default"], no provider_id

                                let filename = format!("{}.yaml", sandbox_product.id);
                                let file_path = catalog_dir.join(&filename);

                                let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                                    &sandbox_product,
                                    Some("Product"),
                                    Some("v1"),
                                )?;

                                fs::write(&file_path, yaml_content).map_err(|e| {
                                    format!("Failed to write sandbox product file: {}", e)
                                })?;

                                println!("  📥 Saved sandbox-only product: {}", filename);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️  Warning: Failed to fetch sandbox catalog: {}", e);
                        eprintln!("    Continuing with production data only...");
                    }
                }
            } else {
                eprintln!(
                    "⚠️  Warning: Sandbox provider configured but STRIPE_SANDBOX_SECRET_KEY not found"
                );
                eprintln!("    Skipping sandbox matching...");
            }
        }

        println!("\nAnalyzing changes...");

        let mut pull_count = 0;
        let mut push_count = 0;
        let mut create_count = 0;
        let mut no_change_count = 0;

        let mut products_to_push: Vec<(&Product, &Product)> = Vec::new(); // (local, remote)

        // Compare each remote product with local version
        for remote_product in &catalog.products {
            let local_product = local_products.get(&remote_product.id);
            let action = determine_sync_action(local_product, remote_product);

            match action {
                SyncAction::Pull => {
                    // Remote is newer or has changes - overwrite local
                    // But preserve sandboxes from local if they exist
                    let mut product_to_save = remote_product.clone();
                    if let Some(local) = local_product {
                        // Merge sandboxes from local into the remote product
                        for (sandbox_name, sandbox_id) in &local.sandboxes {
                            product_to_save
                                .sandboxes
                                .insert(sandbox_name.clone(), sandbox_id.clone());
                        }
                    }

                    let filename = format!("{}.yaml", product_to_save.id);
                    let file_path = catalog_dir.join(&filename);

                    let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                        &product_to_save,
                        Some("Product"),
                        Some("v1"),
                    )?;

                    fs::write(&file_path, yaml_content).map_err(|e| {
                        format!("Failed to write file {}: {}", file_path.display(), e)
                    })?;

                    println!("  ⬇  {} (pulled from remote)", filename);
                    pull_count += 1;
                }
                SyncAction::Push => {
                    // Local has changes - need to push to remote
                    if let Some(local) = local_product {
                        products_to_push.push((local, remote_product));
                        push_count += 1;
                    }
                }
                SyncAction::Create => {
                    // New product from remote
                    let filename = format!("{}.yaml", remote_product.id);
                    let file_path = catalog_dir.join(&filename);

                    let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                        &remote_product,
                        Some("Product"),
                        Some("v1"),
                    )?;

                    fs::write(&file_path, yaml_content).map_err(|e| {
                        format!("Failed to write file {}: {}", file_path.display(), e)
                    })?;

                    println!("  ✨ {} (new from remote)", filename);
                    create_count += 1;
                }
                SyncAction::NoChange => {
                    no_change_count += 1;
                }
            }
        }

        println!("\n📊 Sync Summary:");
        if create_count > 0 {
            println!("  ✨ {} new products", create_count);
        }
        if pull_count > 0 {
            println!("  ⬇  {} products pulled from remote", pull_count);
        }
        if no_change_count > 0 {
            println!("  ✓  {} products unchanged", no_change_count);
        }
        if push_count > 0 {
            println!(
                "  ⚠️  {} products with local changes need to be pushed",
                push_count
            );

            // Handle push workflow interactively
            self.handle_push_workflow(products_to_push, &provider_config, ctx, &catalog_dir)
                .await?;
        }

        // Check for sandbox-only products that need to be created in production
        let sandbox_only_products: Vec<&Product> = local_products
            .values()
            .filter(|p| p.has_sandbox("default") && p.deployed_id.is_none())
            .collect();

        if !sandbox_only_products.is_empty() {
            println!(
                "\n🆕 Found {} sandbox-only products that can be created in production",
                sandbox_only_products.len()
            );

            self.handle_create_workflow(
                sandbox_only_products,
                &provider_config,
                &api_key,
                &catalog_dir,
            )
            .await?;
        }

        // Sync meters
        println!("\n📊 Syncing meters...");
        self.sync_meters(&api_key, &provider_name, &metering_dir)
            .await?;

        Ok(())
    }

    /// Sync meters from Stripe to local YAML files
    async fn sync_meters(
        &self,
        api_key: &str,
        provider_name: &str,
        metering_dir: &std::path::Path,
    ) -> Result<(), String> {
        use moneymq_driver_stripe::download_meters;

        // Download production meters from Stripe
        let mut meter_collection = download_meters(api_key, provider_name, true)
            .await
            .map_err(|e| format!("Failed to download meters: {}", e))?;

        println!(
            "✓ Downloaded {} meters from production",
            meter_collection.total_count
        );

        // Check for sandbox meters if sandbox is configured
        let sandbox_api_key = env::var("STRIPE_SANDBOX_SECRET_KEY").ok();
        if let Some(sandbox_key) = sandbox_api_key {
            println!("Fetching sandbox meters to match...");

            match download_meters(&sandbox_key, &format!("{}_sandbox", provider_name), false).await
            {
                Ok(sandbox_meter_collection) => {
                    println!(
                        "✓ Downloaded {} meters from sandbox",
                        sandbox_meter_collection.total_count
                    );

                    // Track which sandbox meters were matched
                    let mut matched_sandbox_ids = std::collections::HashSet::new();

                    // Match sandbox meters to production meters by event_name
                    let mut matched_count = 0;
                    for prod_meter in &mut meter_collection.meters {
                        if let Some(sandbox_meter) = sandbox_meter_collection
                            .meters
                            .iter()
                            .find(|sm| sm.event_name == prod_meter.event_name)
                        {
                            // Sandbox meter has ID in sandboxes["default"]
                            if let Some(sandbox_id) = sandbox_meter.sandboxes.get("default") {
                                prod_meter
                                    .sandboxes
                                    .insert("default".to_string(), sandbox_id.clone());
                                matched_count += 1;
                                matched_sandbox_ids.insert(sandbox_id.clone());
                            }
                        }
                    }

                    if matched_count > 0 {
                        println!(
                            "✓ Matched {} meters with sandbox equivalents",
                            matched_count
                        );
                    }

                    // Find sandbox-only meters (not in production)
                    let sandbox_only: Vec<_> = sandbox_meter_collection
                        .meters
                        .into_iter()
                        .filter(|sm| {
                            sm.sandboxes
                                .get("default")
                                .map(|id| !matched_sandbox_ids.contains(id))
                                .unwrap_or(false)
                        })
                        .collect();

                    if !sandbox_only.is_empty() {
                        println!(
                            "✓ Found {} sandbox-only meters (not in production)",
                            sandbox_only.len()
                        );

                        // Add sandbox-only meters to the collection
                        for mut sandbox_meter in sandbox_only {
                            // Clear deployed_id since it doesn't exist in production
                            sandbox_meter.deployed_id = None;
                            meter_collection.meters.push(sandbox_meter);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  Warning: Failed to fetch sandbox meters: {}", e);
                    eprintln!("    Continuing with production data only...");
                }
            }
        }

        // Create metering directory if it doesn't exist
        if !metering_dir.exists() {
            fs::create_dir_all(metering_dir)
                .map_err(|e| format!("Failed to create metering directory: {}", e))?;
            println!("✓ Created metering directory: {}", metering_dir.display());
        }

        // Save each meter as a YAML file
        for meter in &meter_collection.meters {
            let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                &meter,
                Some("Meter"),
                Some("v1"),
            )?;

            let filename = format!("{}.yaml", meter.id);
            let file_path = metering_dir.join(&filename);

            fs::write(&file_path, yaml_content)
                .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;
        }

        println!(
            "✓ Saved {} meter files to {}\n",
            meter_collection.meters.len(),
            metering_dir.display()
        );

        Ok(())
    }

    /// Handle the interactive push workflow for products with local changes
    async fn handle_push_workflow(
        &self,
        products_to_push: Vec<(&Product, &Product)>,
        provider_config: &crate::manifest::ProviderConfig,
        ctx: &Context,
        catalog_dir: &std::path::Path,
    ) -> Result<(), String> {
        println!("\n🔍 Reviewing products with local changes...\n");

        for (local, remote) in products_to_push {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            display_diff(local, remote);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

            let stripe_config = provider_config.stripe_config().ok_or_else(|| {
                format!(
                    "Provider '{}' is type {}, but this command requires a Stripe provider",
                    ctx.provider, provider_config
                )
            })?;
            // Check if we have a "default" sandbox configured in the manifest
            let has_sandbox_config = stripe_config.sandboxes.contains_key("default");
            let has_sandbox_key = env::var("STRIPE_SANDBOX_SECRET_KEY").is_ok();

            if has_sandbox_config && has_sandbox_key {
                // Handle sandbox product
                if let Some(sandbox_id) = local.get_sandbox_id("default") {
                    // Sandbox product exists - update it
                    let sandbox_url =
                        format!("https://dashboard.stripe.com/test/products/{}", sandbox_id);

                    println!("🧪 Sandbox Update");
                    println!("   URL: {}", sandbox_url);
                    println!();

                    let confirm_sandbox = Confirm::new()
                        .with_prompt("Update sandbox product?")
                        .default(true)
                        .interact()
                        .map_err(|e| format!("Failed to get user input: {}", e))?;

                    if confirm_sandbox {
                        // Get sandbox API key from environment
                        let sandbox_api_key =
                            env::var("STRIPE_SANDBOX_SECRET_KEY").map_err(|_| {
                                "Sandbox API key not found in STRIPE_SANDBOX_SECRET_KEY".to_string()
                            })?;

                        println!("  ⏳ Updating sandbox...");
                        update_product(&sandbox_api_key, sandbox_id, local)
                            .await
                            .map_err(|e| format!("Failed to update sandbox product: {}", e))?;

                        println!("  ✅ Sandbox updated successfully!");
                        println!("  🔗 View at: {}", sandbox_url);
                        println!();
                    } else {
                        println!("  ⏭️  Skipped sandbox update\n");
                        continue;
                    }
                } else {
                    // Sandbox product doesn't exist - prompt to create
                    println!("🧪 Sandbox Product Missing");
                    println!("   This product exists in production but not in sandbox.");
                    println!();

                    let create_sandbox = Confirm::new()
                        .with_prompt("Create this product in sandbox?")
                        .default(true)
                        .interact()
                        .map_err(|e| format!("Failed to get user input: {}", e))?;

                    if create_sandbox {
                        println!("  ⚠️  Note: Creating products via API is not yet implemented.");
                        println!("  📝 Please create the product manually in Stripe Dashboard:");
                        println!("      https://dashboard.stripe.com/test/products");
                        println!();
                        println!(
                            "  💡 Tip: After creating, run 'moneymq catalog sync' again to link it."
                        );
                        println!();
                    }
                }
            }

            // Update production
            if let Some(deployed_id) = &local.deployed_id {
                let production_url =
                    format!("https://dashboard.stripe.com/products/{}", deployed_id);

                println!("🚀 Production Update");
                println!("   URL: {}", production_url);
                println!();

                let confirm_production = Confirm::new()
                    .with_prompt("Update production product? (This affects live data!)")
                    .default(false)
                    .interact()
                    .map_err(|e| format!("Failed to get user input: {}", e))?;

                if confirm_production {
                    // Get production API key
                    let production_api_key = match &self.api_key {
                        Some(key) => key.clone(),
                        None => match env::var("STRIPE_SECRET_KEY") {
                            Ok(key) => key,
                            Err(_) => stripe_config
                                .api_key
                                .as_ref()
                                .ok_or_else(|| format!("Production API key not found"))?
                                .clone(),
                        },
                    };

                    println!("  ⏳ Updating production...");
                    update_product(&production_api_key, deployed_id, local)
                        .await
                        .map_err(|e| format!("Failed to update production product: {}", e))?;

                    println!("  ✅ Production updated successfully!");
                    println!("  🔗 View at: {}", production_url);
                    println!();

                    // Update local file with new timestamp
                    let filename = format!("{}.yaml", local.id);
                    let file_path = catalog_dir.join(&filename);

                    // Re-fetch the updated product to get new timestamp
                    let updated_catalog =
                        download_catalog(&production_api_key, &ctx.provider, true)
                            .await
                            .map_err(|e| format!("Failed to fetch updated catalog: {}", e))?;

                    if let Some(updated_product) =
                        updated_catalog.products.iter().find(|p| p.id == local.id)
                    {
                        // Preserve sandboxes from local
                        let mut product_to_save = updated_product.clone();
                        for (sandbox_name, sandbox_id) in &local.sandboxes {
                            product_to_save
                                .sandboxes
                                .insert(sandbox_name.clone(), sandbox_id.clone());
                        }

                        let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                            &product_to_save,
                            Some("Product"),
                            Some("v1"),
                        )?;

                        fs::write(&file_path, yaml_content).map_err(|e| {
                            format!(
                                "Failed to write updated file {}: {}",
                                file_path.display(),
                                e
                            )
                        })?;

                        println!("  💾 Local file updated with new timestamp");
                        println!();
                    }
                } else {
                    println!("  ⏭️  Skipped production update\n");
                }
            }
        }

        Ok(())
    }

    /// Handle the creation workflow for sandbox-only products
    async fn handle_create_workflow(
        &self,
        products: Vec<&Product>,
        _provider_config: &crate::manifest::ProviderConfig,
        production_api_key: &str,
        catalog_dir: &std::path::Path,
    ) -> Result<(), String> {
        use dialoguer::Confirm;
        use moneymq_driver_stripe::create_product;

        println!("\n🔍 Reviewing sandbox-only products...\n");

        for product in products {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("📦 {}", product.name.as_deref().unwrap_or("(unnamed)"));
            println!();

            if let Some(description) = &product.description {
                println!("Description: {}", description);
            }

            println!("Active: {}", product.active);

            if let Some(sandbox_id) = product.get_sandbox_id("default") {
                println!("Sandbox ID: {}", sandbox_id);
                println!(
                    "Sandbox URL: https://dashboard.stripe.com/test/products/{}",
                    sandbox_id
                );
            }

            println!();

            // Ask if user wants to create this product in production
            let create = Confirm::new()
                .with_prompt("Create this product in production?")
                .default(false)
                .interact()
                .map_err(|e| format!("Failed to get confirmation: {}", e))?;

            if !create {
                println!("  ⏭️  Skipped\n");
                continue;
            }

            // Create the product in production
            println!("  ⏳ Creating product in production...");

            let production_id = create_product(production_api_key, product)
                .await
                .map_err(|e| format!("Failed to create product: {}", e))?;

            println!("  ✅ Product created successfully!");
            println!("  🔗 Production ID: {}", production_id);
            println!(
                "  🔗 View at: https://dashboard.stripe.com/products/{}",
                production_id
            );

            // Update local file with production deployed_id
            let mut updated_product = product.clone();
            updated_product.deployed_id = Some(production_id);

            let filename = format!("{}.yaml", updated_product.id);
            let file_path = catalog_dir.join(&filename);

            let yaml_content = crate::yaml_util::to_pretty_yaml_with_header(
                &updated_product,
                Some("Product"),
                Some("v1"),
            )?;

            fs::write(&file_path, yaml_content)
                .map_err(|e| format!("Failed to write updated file: {}", e))?;

            println!("  💾 Local file updated with production ID\n");
        }

        Ok(())
    }
}
