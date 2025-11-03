use std::{collections::HashMap, env, fs};

use clap::Parser;
use console::style;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
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
        "  {} {} {}",
        style("Product:").bold(),
        style(local.name.as_deref().unwrap_or("Unnamed"))
            .cyan()
            .bold(),
        style(format!("({})", &local.id)).dim()
    );
    println!();

    if local.name != remote.name {
        println!("  {}", style("Name:").bold());
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
        println!("  {}", style("Description:").bold());

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
        println!("  {}", style("Active:").bold());
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
        println!("  {}", style("Product Type:").bold());
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

        println!("  {}", style("Metadata:").bold());

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
        let loaded_count = if catalog_dir.exists() {
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
                            eprintln!(
                                "{} Failed to parse {}: {}",
                                style("Warning:").yellow(),
                                path.display(),
                                e
                            );
                            eprintln!(
                                "  {} You may need to regenerate it.",
                                style("Skipping this file.").dim()
                            );
                            continue;
                        }
                    }
                }
            }

            local_products.len()
        } else {
            // Create the catalog directory if it doesn't exist
            fs::create_dir_all(&catalog_dir)
                .map_err(|e| format!("Failed to create catalog directory: {}", e))?;
            0
        };

        // Check if workspace is empty (no local products loaded)
        let is_initial_sync = loaded_count == 0;

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

        // Always sync from production (is_production = true)
        let is_production = !stripe_config.test_mode;

        // Download the production catalog with spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        spinner.set_message("Downloading production catalog...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let mut catalog = download_catalog(&api_key, &provider_name, is_production)
            .await
            .map_err(|e| format!("Failed to download catalog: {}", e))?;

        spinner.finish_and_clear();
        let downloaded_count = catalog.total_count;

        let mut matched_count = 0;
        let mut sandbox_only_count = 0;

        // Check if we have a "default" sandbox configuration in the manifest
        if let Some(_sandbox_config) = stripe_config.sandboxes.get("default") {
            // Try to get sandbox API key from environment
            let sandbox_api_key = env::var("STRIPE_SANDBOX_SECRET_KEY").ok();

            if let Some(sandbox_key) = sandbox_api_key {
                let spinner = ProgressBar::new_spinner();
                spinner.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                spinner.set_message("Downloading sandbox catalog...");
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                match download_catalog(&sandbox_key, &format!("{}_sandbox", provider_name), false)
                    .await
                {
                    Ok(sandbox_catalog) => {
                        spinner.finish_and_clear();
                        // Track which sandbox products were matched
                        let mut matched_sandbox_ids = std::collections::HashSet::new();

                        // Match sandbox products to production products by name
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

                                // Also match prices within the products
                                for prod_price in &mut prod_product.prices {
                                    // Match by currency, unit_amount, and interval (for recurring prices)
                                    // Only set sandbox ID if the price actually exists in sandbox
                                    if let Some(sandbox_price) = sandbox_product.prices.iter().find(|sp| {
                                        sp.currency == prod_price.currency
                                            && sp.unit_amount == prod_price.unit_amount
                                            && sp.recurring_interval == prod_price.recurring_interval
                                            && sp.recurring_interval_count == prod_price.recurring_interval_count
                                            && sp.pricing_type == prod_price.pricing_type
                                    }) {
                                        // Copy sandbox ID from sandbox price to production price
                                        // This means the price exists in both production and sandbox
                                        if let Some(price_sandbox_id) = sandbox_price.sandboxes.get("default") {
                                            prod_price
                                                .sandboxes
                                                .insert("default".to_string(), price_sandbox_id.clone());
                                        }
                                    }
                                    // If no matching sandbox price found, prod_price.sandboxes remains empty
                                    // which correctly indicates the price doesn't exist in sandbox yet
                                }
                            }
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

                        sandbox_only_count = sandbox_only.len();

                        // Save sandbox-only products to disk
                        for sandbox_product in sandbox_only {
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
                        }
                    }
                    Err(e) => {
                        spinner.finish_and_clear();
                        eprintln!(
                            "{} Failed to fetch sandbox catalog: {}",
                            style("Warning:").yellow(),
                            e
                        );
                        eprintln!("  {}", style("Continuing with production data only").dim());
                    }
                }
            } else {
                eprintln!(
                    "{} Sandbox provider configured but STRIPE_SANDBOX_SECRET_KEY not found",
                    style("Warning:").yellow()
                );
                eprintln!("  {}", style("Skipping sandbox matching").dim());
            }
        }

        // Print condensed summary
        if is_initial_sync {
            println!(
                "{} Initial sync: {} products downloaded{}",
                style("✓").green(),
                downloaded_count,
                if matched_count > 0 {
                    format!(", {} matched with sandbox", matched_count)
                } else {
                    String::new()
                }
            );
        } else {
            println!(
                "{} {} loaded, {} downloaded{}",
                style("✓").green(),
                loaded_count,
                downloaded_count,
                if matched_count > 0 {
                    format!(", {} matched with sandbox", matched_count)
                } else {
                    String::new()
                }
            );
        }

        let mut _pull_count = 0;
        let mut push_count = 0;
        let mut _create_count = 0;
        let mut no_change_count = 0;

        let mut products_to_push: Vec<(&Product, &Product)> = Vec::new(); // (local, remote)

        // Compare each remote product with local version
        for remote_product in &catalog.products {
            let local_product = local_products.get(&remote_product.id);
            let action = determine_sync_action(local_product, remote_product);

            match action {
                SyncAction::Pull => {
                    // Remote is newer or has changes - overwrite local
                    // But preserve sandboxes from local if they exist (both product and price level)
                    let mut product_to_save = remote_product.clone();
                    if let Some(local) = local_product {
                        // Merge product-level sandboxes from local into the remote product
                        for (sandbox_name, sandbox_id) in &local.sandboxes {
                            product_to_save
                                .sandboxes
                                .insert(sandbox_name.clone(), sandbox_id.clone());
                        }

                        // Merge price-level sandboxes from local into the remote prices
                        // Match by price attributes, not ID (since IDs differ between prod/sandbox)
                        for price in &mut product_to_save.prices {
                            if let Some(local_price) = local.prices.iter().find(|lp| {
                                lp.currency == price.currency
                                    && lp.unit_amount == price.unit_amount
                                    && lp.recurring_interval == price.recurring_interval
                                    && lp.recurring_interval_count == price.recurring_interval_count
                                    && lp.pricing_type == price.pricing_type
                            }) {
                                for (sandbox_name, sandbox_id) in &local_price.sandboxes {
                                    price.sandboxes.insert(sandbox_name.clone(), sandbox_id.clone());
                                }
                            }
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

                    _pull_count += 1;
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

                    _create_count += 1;
                }
                SyncAction::NoChange => {
                    // Check if sandboxes have changed (product-level or price-level)
                    let sandboxes_changed = match local_product {
                        Some(local) => {
                            // Check product-level sandboxes
                            let product_sandboxes_changed = local.sandboxes != remote_product.sandboxes;

                            // Check price-level sandboxes
                            // Match by price attributes, not ID (since IDs differ between prod/sandbox)
                            let price_sandboxes_changed = local.prices.iter().any(|local_price| {
                                // Find matching remote price by attributes
                                remote_product.prices.iter()
                                    .find(|rp| {
                                        rp.currency == local_price.currency
                                            && rp.unit_amount == local_price.unit_amount
                                            && rp.recurring_interval == local_price.recurring_interval
                                            && rp.recurring_interval_count == local_price.recurring_interval_count
                                            && rp.pricing_type == local_price.pricing_type
                                    })
                                    .map(|remote_price| remote_price.sandboxes != local_price.sandboxes)
                                    .unwrap_or(false)
                            });

                            product_sandboxes_changed || price_sandboxes_changed
                        }
                        None => {
                            // Check if remote has any sandbox info
                            !remote_product.sandboxes.is_empty()
                                || remote_product.prices.iter().any(|p| !p.sandboxes.is_empty())
                        }
                    };

                    if sandboxes_changed {
                        // Sandboxes have changed, need to update the file
                        // Preserve deployed_id from local if it exists
                        let mut product_to_save = remote_product.clone();

                        if let Some(local) = local_product {
                            // Preserve deployed_id from local (in case remote doesn't have it)
                            if product_to_save.deployed_id.is_none() && local.deployed_id.is_some() {
                                product_to_save.deployed_id = local.deployed_id.clone();
                            }

                            // Merge price-level sandboxes and deployed_ids from local
                            for price in &mut product_to_save.prices {
                                if let Some(local_price) = local.prices.iter().find(|lp| {
                                    lp.currency == price.currency
                                        && lp.unit_amount == price.unit_amount
                                        && lp.recurring_interval == price.recurring_interval
                                        && lp.recurring_interval_count == price.recurring_interval_count
                                        && lp.pricing_type == price.pricing_type
                                }) {
                                    // Preserve deployed_id from local price
                                    if price.deployed_id.is_none() && local_price.deployed_id.is_some() {
                                        price.deployed_id = local_price.deployed_id.clone();
                                    }

                                    // Merge sandbox IDs from local price
                                    for (sandbox_name, sandbox_id) in &local_price.sandboxes {
                                        price.sandboxes.insert(sandbox_name.clone(), sandbox_id.clone());
                                    }
                                }
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
                    }

                    no_change_count += 1;
                }
            }
        }

        // Print condensed sync summary
        if is_initial_sync {
            println!(
                "{} {} products saved to {}",
                style("✓").green(),
                _create_count,
                catalog_dir.display()
            );
        } else {
            println!(
                "{} {} unchanged{}{}",
                style("✓").green(),
                no_change_count,
                if push_count > 0 {
                    format!(", {} with local changes", push_count)
                } else {
                    String::new()
                },
                if sandbox_only_count > 0 {
                    format!(", {} sandbox-only", sandbox_only_count)
                } else {
                    String::new()
                }
            );
        }

        if push_count > 0 {
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
            println!();
            println!(
                "{} Found {} sandbox-only products that can be created in production",
                style("Note:").yellow(),
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
        println!("{} Syncing meters", style("»").dim());
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

        // Download production meters from Stripe with spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        spinner.set_message("Downloading production meters...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let mut meter_collection = download_meters(api_key, provider_name, true)
            .await
            .map_err(|e| format!("Failed to download meters: {}", e))?;

        spinner.finish_and_clear();

        let mut matched_count = 0;
        let mut sandbox_only_count = 0;

        // Check for sandbox meters if sandbox is configured
        let sandbox_api_key = env::var("STRIPE_SANDBOX_SECRET_KEY").ok();
        if let Some(sandbox_key) = sandbox_api_key {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            spinner.set_message("Downloading sandbox meters...");
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));

            match download_meters(&sandbox_key, &format!("{}_sandbox", provider_name), false).await
            {
                Ok(sandbox_meter_collection) => {
                    spinner.finish_and_clear();

                    // Track which sandbox meters were matched
                    let mut matched_sandbox_ids = std::collections::HashSet::new();

                    // Match sandbox meters to production meters by event_name
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

                    sandbox_only_count = sandbox_only.len();

                    // Add sandbox-only meters to the collection
                    for mut sandbox_meter in sandbox_only {
                        // Clear deployed_id since it doesn't exist in production
                        sandbox_meter.deployed_id = None;
                        meter_collection.meters.push(sandbox_meter);
                    }
                }
                Err(e) => {
                    spinner.finish_and_clear();
                    eprintln!(
                        "{} Failed to fetch sandbox meters: {}",
                        style("Warning:").yellow(),
                        e
                    );
                    eprintln!("  {}", style("Continuing with production data only").dim());
                }
            }
        }

        // Create metering directory if it doesn't exist
        if !metering_dir.exists() {
            fs::create_dir_all(metering_dir)
                .map_err(|e| format!("Failed to create metering directory: {}", e))?;
        }

        // Save each meter as a YAML file
        for meter in &meter_collection.meters {
            let yaml_content =
                crate::yaml_util::to_pretty_yaml_with_header(&meter, Some("Meter"), Some("v1"))?;

            let filename = format!("{}.yaml", meter.id);
            let file_path = metering_dir.join(&filename);

            fs::write(&file_path, yaml_content)
                .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;
        }

        println!(
            "{} {} meters{}{}",
            style("✓").green(),
            meter_collection.meters.len(),
            if matched_count > 0 {
                format!(", {} matched with sandbox", matched_count)
            } else {
                String::new()
            },
            if sandbox_only_count > 0 {
                format!(", {} sandbox-only", sandbox_only_count)
            } else {
                String::new()
            }
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
        let stripe_config = provider_config.stripe_config().ok_or_else(|| {
            format!(
                "Provider '{}' is type {}, but this command requires a Stripe provider",
                ctx.provider, provider_config
            )
        })?;

        // Check if we have sandbox configured
        let has_sandbox_config = stripe_config.sandboxes.contains_key("default");
        let has_sandbox_key = env::var("STRIPE_SANDBOX_SECRET_KEY").is_ok();

        // First, display all changes
        println!();
        println!("{} Reviewing products with local changes", style("»").dim());
        println!();

        for (local, remote) in &products_to_push {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            display_diff(local, remote);
        }
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!();

        // Batch 1: Handle all sandbox updates
        if has_sandbox_config && has_sandbox_key {
            let products_with_sandbox: Vec<_> = products_to_push
                .iter()
                .filter(|(local, _)| local.has_sandbox("default"))
                .collect();

            let products_without_sandbox: Vec<_> = products_to_push
                .iter()
                .filter(|(local, _)| !local.has_sandbox("default"))
                .collect();

            if !products_with_sandbox.is_empty() {
                // Count prices that need to be created/updated in sandbox
                let mut prices_to_create = 0;
                let mut prices_to_update = 0;

                for (local, _) in &products_with_sandbox {
                    for price in &local.prices {
                        if price.has_sandbox("default") {
                            prices_to_update += 1;
                        } else {
                            prices_to_create += 1;
                        }
                    }
                }

                println!(
                    "{} {} products will be updated in sandbox",
                    style("Sandbox").yellow(),
                    products_with_sandbox.len()
                );

                if prices_to_create > 0 || prices_to_update > 0 {
                    println!(
                        "  {} {} prices ({} new, {} updates)",
                        style("Prices:").dim(),
                        prices_to_create + prices_to_update,
                        prices_to_create,
                        prices_to_update
                    );
                }

                println!();

                let confirm_sandbox = Confirm::new()
                    .with_prompt("Deploy all changes to sandbox?")
                    .default(true)
                    .interact()
                    .map_err(|e| format!("Failed to get user input: {}", e))?;

                if confirm_sandbox {
                    let sandbox_api_key = env::var("STRIPE_SANDBOX_SECRET_KEY").map_err(|_| {
                        "Sandbox API key not found in STRIPE_SANDBOX_SECRET_KEY".to_string()
                    })?;

                    let spinner = ProgressBar::new(products_with_sandbox.len() as u64);
                    spinner.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{bar:40.green/dim}] {pos}/{len} {msg}")
                            .unwrap()
                            .progress_chars("=> "),
                    );

                    for (local, _remote) in &products_with_sandbox {
                        if let Some(sandbox_id) = local.get_sandbox_id("default") {
                            spinner.set_message(format!(
                                "Updating {}",
                                local.name.as_deref().unwrap_or("product")
                            ));

                            update_product(&sandbox_api_key, sandbox_id, local)
                                .await
                                .map_err(|e| format!("Failed to update sandbox product: {}", e))?;

                            spinner.inc(1);
                        }
                    }

                    spinner.finish_and_clear();
                    println!(
                        "{} {} products updated in sandbox",
                        style("✓").green(),
                        products_with_sandbox.len()
                    );
                    println!();

                    // Handle price creation in sandbox
                    if prices_to_create > 0 {
                        use moneymq_driver_stripe::create_price;

                        println!(
                            "{} Creating {} prices in sandbox",
                            style("»").dim(),
                            prices_to_create
                        );

                        let spinner = ProgressBar::new(prices_to_create as u64);
                        spinner.set_style(
                            ProgressStyle::default_bar()
                                .template("{spinner:.green} [{bar:40.green/dim}] {pos}/{len} {msg}")
                                .unwrap()
                                .progress_chars("=> "),
                        );

                        for (local, _) in &products_with_sandbox {
                            if let Some(sandbox_product_id) = local.get_sandbox_id("default") {
                                for price in &local.prices {
                                    if !price.has_sandbox("default") {
                                        let amount = price.unit_amount
                                            .map(|a| format!("${}.{:02}", a / 100, a % 100))
                                            .unwrap_or_else(|| "custom".to_string());

                                        spinner.set_message(format!("Creating price {}", amount));

                                        match create_price(&sandbox_api_key, sandbox_product_id, price).await {
                                            Ok(_price_id) => {
                                                // Price created successfully
                                                // Note: We'll need to re-sync to capture the new price ID in the YAML
                                                spinner.inc(1);
                                            }
                                            Err(e) => {
                                                spinner.finish_and_clear();
                                                eprintln!(
                                                    "{} Failed to create price: {}",
                                                    style("Warning:").yellow(),
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        spinner.finish_and_clear();
                        println!(
                            "{} {} prices created in sandbox",
                            style("✓").green(),
                            prices_to_create
                        );
                        println!(
                            "  {} Run 'catalog sync' again to capture new price IDs",
                            style("Note:").dim()
                        );
                        println!();
                    }
                } else {
                    println!("{} Skipped sandbox updates", style("✓").green());
                    println!();
                }
            }

            // Inform about products without sandbox
            if !products_without_sandbox.is_empty() {
                println!(
                    "{} {} products don't have sandbox equivalents (skipping sandbox push)",
                    style("Note:").yellow(),
                    products_without_sandbox.len()
                );
                println!(
                    "  {} Create them manually at: {}",
                    style("Tip:").dim(),
                    style("https://dashboard.stripe.com/test/products").dim()
                );
                println!();
            }
        }

        // Batch 2: Handle all production updates
        let products_with_deployed_id: Vec<_> = products_to_push
            .iter()
            .filter(|(local, _)| local.deployed_id.is_some())
            .collect();

        if !products_with_deployed_id.is_empty() {
            // Count prices that need to be created/updated in production
            let mut prices_to_create = 0;
            let mut prices_to_update = 0;

            for (local, _) in &products_with_deployed_id {
                for price in &local.prices {
                    if price.deployed_id.is_some() {
                        prices_to_update += 1;
                    } else {
                        prices_to_create += 1;
                    }
                }
            }

            println!(
                "{} {} products can be updated in production",
                style("Production").yellow(),
                products_with_deployed_id.len()
            );

            if prices_to_create > 0 || prices_to_update > 0 {
                println!(
                    "  {} {} prices ({} new, {} updates)",
                    style("Prices:").dim(),
                    prices_to_create + prices_to_update,
                    prices_to_create,
                    prices_to_update
                );
            }

            println!();

            let confirm_production = Confirm::new()
                .with_prompt("Deploy all changes to production? (This affects live data!)")
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

                let spinner = ProgressBar::new(products_with_deployed_id.len() as u64);
                spinner.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} [{bar:40.green/dim}] {pos}/{len} {msg}")
                        .unwrap()
                        .progress_chars("=> "),
                );

                for (local, _remote) in &products_with_deployed_id {
                    if let Some(deployed_id) = &local.deployed_id {
                        spinner.set_message(format!(
                            "Updating {}",
                            local.name.as_deref().unwrap_or("product")
                        ));

                        update_product(&production_api_key, deployed_id, local)
                            .await
                            .map_err(|e| format!("Failed to update production product: {}", e))?;

                        spinner.inc(1);
                    }
                }

                spinner.finish_and_clear();

                println!(
                    "{} {} products updated in production",
                    style("✓").green(),
                    products_with_deployed_id.len()
                );
                println!();

                // Handle price creation in production
                if prices_to_create > 0 {
                    use moneymq_driver_stripe::create_price;

                    println!(
                        "{} Creating {} prices in production",
                        style("»").dim(),
                        prices_to_create
                    );

                    let spinner = ProgressBar::new(prices_to_create as u64);
                    spinner.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{bar:40.green/dim}] {pos}/{len} {msg}")
                            .unwrap()
                            .progress_chars("=> "),
                    );

                    for (local, _) in &products_with_deployed_id {
                        if let Some(production_product_id) = &local.deployed_id {
                            for price in &local.prices {
                                if price.deployed_id.is_none() {
                                    let amount = price.unit_amount
                                        .map(|a| format!("${}.{:02}", a / 100, a % 100))
                                        .unwrap_or_else(|| "custom".to_string());

                                    spinner.set_message(format!("Creating price {}", amount));

                                    match create_price(&production_api_key, production_product_id, price).await {
                                        Ok(_price_id) => {
                                            // Price created successfully
                                            // Note: We'll need to re-sync to capture the new price ID in the YAML
                                            spinner.inc(1);
                                        }
                                        Err(e) => {
                                            spinner.finish_and_clear();
                                            eprintln!(
                                                "{} Failed to create price: {}",
                                                style("Warning:").yellow(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    spinner.finish_and_clear();
                    println!(
                        "{} {} prices created in production",
                        style("✓").green(),
                        prices_to_create
                    );
                    println!(
                        "  {} Run 'catalog sync' again to capture new price IDs",
                        style("Note:").dim()
                    );
                    println!();
                }

                // Update all local files with new timestamps
                let spinner = ProgressBar::new_spinner();
                spinner.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                spinner.set_message("Refreshing local catalog...");
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                let updated_catalog = download_catalog(&production_api_key, &ctx.provider, true)
                    .await
                    .map_err(|e| format!("Failed to fetch updated catalog: {}", e))?;

                spinner.finish_and_clear();

                for (local, _remote) in products_with_deployed_id {
                    if let Some(updated_product) =
                        updated_catalog.products.iter().find(|p| p.id == local.id)
                    {
                        // Preserve sandboxes from local (both product and price level)
                        let mut product_to_save = updated_product.clone();
                        for (sandbox_name, sandbox_id) in &local.sandboxes {
                            product_to_save
                                .sandboxes
                                .insert(sandbox_name.clone(), sandbox_id.clone());
                        }

                        // Preserve price-level sandboxes from local
                        for price in &mut product_to_save.prices {
                            if let Some(local_price) = local.prices.iter().find(|lp| {
                                lp.currency == price.currency
                                    && lp.unit_amount == price.unit_amount
                                    && lp.recurring_interval == price.recurring_interval
                                    && lp.recurring_interval_count == price.recurring_interval_count
                                    && lp.pricing_type == price.pricing_type
                            }) {
                                for (sandbox_name, sandbox_id) in &local_price.sandboxes {
                                    price.sandboxes.insert(sandbox_name.clone(), sandbox_id.clone());
                                }
                            }
                        }

                        let filename = format!("{}.yaml", local.id);
                        let file_path = catalog_dir.join(&filename);

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
                    }
                }

                println!("{} Local files updated", style("✓").green());
                println!();
            } else {
                println!("{} Skipped production updates", style("✓").green());
                println!();
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

        println!();
        println!("{} Reviewing sandbox-only products", style("»").dim());
        println!();

        for product in products {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!(
                "  {} {}",
                style("Product:").bold(),
                style(product.name.as_deref().unwrap_or("(unnamed)"))
                    .cyan()
                    .bold()
            );

            if let Some(description) = &product.description {
                println!("  {} {}", style("Description:").dim(), description);
            }

            println!("  {} {}", style("Active:").dim(), product.active);

            if let Some(sandbox_id) = product.get_sandbox_id("default") {
                println!("  {} {}", style("Sandbox ID:").dim(), sandbox_id);
                println!(
                    "  {} https://dashboard.stripe.com/test/products/{}",
                    style("Sandbox URL:").dim(),
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
                println!("{} Skipped", style("✓").green());
                println!();
                continue;
            }

            // Create the product in production
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            spinner.set_message("Creating product in production...");
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));

            let production_id = create_product(production_api_key, product)
                .await
                .map_err(|e| format!("Failed to create product: {}", e))?;

            spinner.finish_and_clear();

            println!("{} Product created: {}", style("✓").green(), production_id);
            println!(
                "  {} https://dashboard.stripe.com/products/{}",
                style("View at:").dim(),
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

            println!("{} Local file updated", style("✓").green());
            println!();
        }

        Ok(())
    }
}
