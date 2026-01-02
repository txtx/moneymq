//! Text-based YAML synchronization for manifest and product files.
//!
//! This module provides surgical insertion/update of YAML content
//! without destroying comments, formatting, or unrelated sections.

use super::{
    Chain, DeploymentType, IacEnvironmentConfig, IacFacilitatorConfig, IacNetworkEnvConfig,
    KeyManagement, ProductSchema,
};

/// Result of a sync operation
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// The updated file content
    pub content: String,
    /// Whether any changes were made
    pub changed: bool,
    /// Description of what was done
    pub message: String,
}

/// Generate YAML text for an environment entry
pub fn generate_environment_yaml(name: &str, config: &IacEnvironmentConfig) -> String {
    let mut lines = vec![format!("  {}:", name)];

    let deployment_str = match &config.deployment {
        DeploymentType::Sandbox => "Sandbox",
        DeploymentType::SelfHosted => "SelfHosted",
        DeploymentType::CloudHosted => "CloudHosted",
    };
    lines.push(format!("    deployment: {}", deployment_str));

    if let Some(project) = &config.project {
        lines.push(format!("    project: {}", project));
    }

    if let Some(workspace) = &config.workspace {
        lines.push(format!("    workspace: {}", workspace));
    }

    if let Some(binding_address) = &config.binding_address {
        lines.push(format!("    binding_address: {}", binding_address));
    }

    if let Some(port) = config.port {
        lines.push(format!("    port: {}", port));
    }

    if let Some(facilitator) = &config.facilitator {
        append_facilitator_yaml(&mut lines, facilitator);
    }

    if let Some(network) = &config.network {
        append_network_yaml(&mut lines, network);
    }

    lines.join("\n") + "\n"
}

fn append_facilitator_yaml(lines: &mut Vec<String>, facilitator: &IacFacilitatorConfig) {
    lines.push("    facilitator:".to_string());
    if let Some(fee) = facilitator.fee {
        lines.push(format!("      fee: {}", fee));
    }
    if let Some(key_management) = &facilitator.key_management {
        let km_str = match key_management {
            KeyManagement::InMemory => "InMemory",
            KeyManagement::TurnKey => "TurnKey",
        };
        lines.push(format!("      key_management: {}", km_str));
    }
}

fn append_network_yaml(lines: &mut Vec<String>, network: &IacNetworkEnvConfig) {
    lines.push("    network:".to_string());
    if let Some(chain) = &network.chain {
        let chain_str = match chain {
            Chain::Solana => "Solana",
        };
        lines.push(format!("      chain: {}", chain_str));
    }
    if let Some(recipient) = &network.recipient {
        lines.push(format!("      recipient: {}", recipient));
    }
    if let Some(binding_address) = &network.binding_address {
        lines.push(format!("      binding_address: {}", binding_address));
    }
    if let Some(rpc_port) = network.rpc_port {
        lines.push(format!("      rpc_port: {}", rpc_port));
    }
    if let Some(ws_port) = network.ws_port {
        lines.push(format!("      ws_port: {}", ws_port));
    }
    if let Some(rpc_url) = &network.rpc_url {
        lines.push(format!("      rpc_url: {}", rpc_url));
    }
    if let Some(ws_url) = &network.ws_url {
        lines.push(format!("      ws_url: {}", ws_url));
    }
}

/// Find the position to insert a new environment entry.
///
/// Returns the byte position after the last environment entry,
/// or after the "environments:" line if the section is empty.
pub fn find_environments_insert_position(content: &str) -> Option<usize> {
    // Find the "environments:" line (could be at start or after newline)
    let env_start = if content.starts_with("environments:") {
        0
    } else {
        content.find("\nenvironments:")? + 1
    };

    let line_end = content[env_start..].find('\n').map(|p| env_start + p + 1)?;

    // Skip past any existing environment entries and comments
    let rest = &content[line_end..];

    let mut pos = line_end;
    let mut in_env_section = true;

    for line in rest.lines() {
        if line.is_empty() {
            // Empty line - continue
            pos += 1;
            continue;
        }

        if line.starts_with('#') {
            // Comment line - include it
            pos += line.len() + 1;
            continue;
        }

        if !line.starts_with(' ') && !line.starts_with('\t') {
            // Found a new top-level key, stop here
            in_env_section = false;
            break;
        }

        // This is content within the environments section
        pos += line.len() + 1;
    }

    // If we're still in the env section and reached EOF, pos is correct
    if in_env_section {
        Some(pos.min(content.len()))
    } else {
        Some(pos)
    }
}

/// Check if an environment with the given name already exists
pub fn environment_exists(content: &str, name: &str) -> bool {
    // Look for the pattern "  name:" at the start of a line within environments section
    let pattern = format!("\n  {}:", name);
    if content.contains(&pattern) {
        return true;
    }

    // Also check at the very start (unlikely but possible)
    let start_pattern = format!("  {}:", name);
    if content.starts_with(&start_pattern) {
        return true;
    }

    false
}

/// Check if an environments section exists
pub fn has_environments_section(content: &str) -> bool {
    content.contains("\nenvironments:") || content.starts_with("environments:")
}

/// Insert an environment into the manifest content
pub fn insert_environment(content: &str, name: &str, config: &IacEnvironmentConfig) -> SyncResult {
    // Check if environment already exists
    if environment_exists(content, name) {
        return SyncResult {
            content: content.to_string(),
            changed: false,
            message: format!("Environment '{}' already exists (skipped)", name),
        };
    }

    let env_yaml = generate_environment_yaml(name, config);
    let mut new_content = content.to_string();

    if let Some(insert_pos) = find_environments_insert_position(content) {
        new_content.insert_str(insert_pos, &env_yaml);
        SyncResult {
            content: new_content,
            changed: true,
            message: format!("Added environment '{}'", name),
        }
    } else if !has_environments_section(content) {
        // No environments section, append one
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str("\nenvironments:\n");
        new_content.push_str(&env_yaml);
        SyncResult {
            content: new_content,
            changed: true,
            message: format!("Created environments section and added '{}'", name),
        }
    } else {
        SyncResult {
            content: content.to_string(),
            changed: false,
            message: "Failed to find insertion point".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // generate_environment_yaml tests
    // ========================================================================

    #[test]
    fn test_generate_cloud_hosted_env() {
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: Some("My Project".to_string()),
            workspace: Some("my-workspace".to_string()),
            binding_address: None,
            port: None,
            facilitator: Some(IacFacilitatorConfig {
                fee: Some(0),
                key_management: Some(KeyManagement::TurnKey),
            }),
            network: None,
        };

        let yaml = generate_environment_yaml("cloud", &config);

        assert!(yaml.starts_with("  cloud:\n"));
        assert!(yaml.contains("    deployment: CloudHosted\n"));
        assert!(yaml.contains("    project: My Project\n"));
        assert!(yaml.contains("    workspace: my-workspace\n"));
        assert!(yaml.contains("    facilitator:\n"));
        assert!(yaml.contains("      fee: 0\n"));
        assert!(yaml.contains("      key_management: TurnKey\n"));
        assert!(!yaml.contains("network:"));
    }

    #[test]
    fn test_generate_sandbox_env() {
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::Sandbox,
            project: None,
            workspace: None,
            binding_address: Some("0.0.0.0".to_string()),
            port: Some(8488),
            facilitator: Some(IacFacilitatorConfig {
                fee: Some(0),
                key_management: None,
            }),
            network: Some(IacNetworkEnvConfig {
                chain: Some(Chain::Solana),
                recipient: None,
                binding_address: Some("0.0.0.0".to_string()),
                rpc_port: Some(8899),
                ws_port: Some(8900),
                rpc_url: None,
                ws_url: None,
            }),
        };

        let yaml = generate_environment_yaml("sandbox", &config);

        assert!(yaml.contains("  sandbox:\n"));
        assert!(yaml.contains("    deployment: Sandbox\n"));
        assert!(yaml.contains("    binding_address: 0.0.0.0\n"));
        assert!(yaml.contains("    port: 8488\n"));
        assert!(yaml.contains("    network:\n"));
        assert!(yaml.contains("      chain: Solana\n"));
        assert!(yaml.contains("      rpc_port: 8899\n"));
        assert!(yaml.contains("      ws_port: 8900\n"));
    }

    #[test]
    fn test_generate_self_hosted_env() {
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::SelfHosted,
            project: None,
            workspace: None,
            binding_address: Some("0.0.0.0".to_string()),
            port: Some(8488),
            facilitator: Some(IacFacilitatorConfig {
                fee: Some(100),
                key_management: Some(KeyManagement::TurnKey),
            }),
            network: Some(IacNetworkEnvConfig {
                chain: Some(Chain::Solana),
                recipient: Some("abc123".to_string()),
                binding_address: None,
                rpc_port: None,
                ws_port: None,
                rpc_url: Some("https://api.mainnet-beta.solana.com".to_string()),
                ws_url: Some("wss://api.mainnet-beta.solana.com".to_string()),
            }),
        };

        let yaml = generate_environment_yaml("production", &config);

        assert!(yaml.contains("  production:\n"));
        assert!(yaml.contains("    deployment: SelfHosted\n"));
        assert!(yaml.contains("      rpc_url: https://api.mainnet-beta.solana.com\n"));
        assert!(yaml.contains("      ws_url: wss://api.mainnet-beta.solana.com\n"));
        assert!(yaml.contains("      recipient: abc123\n"));
    }

    #[test]
    fn test_generate_minimal_env() {
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: None,
            workspace: Some("test".to_string()),
            binding_address: None,
            port: None,
            facilitator: None,
            network: None,
        };

        let yaml = generate_environment_yaml("cloud", &config);

        assert!(yaml.contains("  cloud:\n"));
        assert!(yaml.contains("    deployment: CloudHosted\n"));
        assert!(yaml.contains("    workspace: test\n"));
        assert!(!yaml.contains("facilitator:"));
        assert!(!yaml.contains("network:"));
    }

    // ========================================================================
    // find_environments_insert_position tests
    // ========================================================================

    #[test]
    fn test_find_position_with_existing_envs() {
        let content = r#"---
catalogs:
  v1:
    description: "Test"

environments:
  sandbox:
    deployment: Sandbox
    port: 8488
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_some());

        let pos = pos.unwrap();
        // Should be at the end of the file
        assert_eq!(pos, content.len());
    }

    #[test]
    fn test_find_position_empty_envs_section() {
        let content = r#"---
catalogs:
  v1:
    description: "Test"

environments:
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_some());

        let pos = pos.unwrap();
        assert_eq!(pos, content.len());
    }

    #[test]
    fn test_find_position_envs_followed_by_section() {
        let content = r#"---
environments:
  sandbox:
    deployment: Sandbox

other_section:
  key: value
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_some());

        let pos = pos.unwrap();
        // Should be before "other_section:"
        let rest = &content[pos..];
        assert!(rest.starts_with("other_section:") || rest.starts_with("\nother_section:"));
    }

    #[test]
    fn test_find_position_with_comments() {
        let content = r#"---
environments:
  # This is a sandbox
  sandbox:
    deployment: Sandbox
  # End of envs
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_some());
    }

    #[test]
    fn test_find_position_no_envs_section() {
        let content = r#"---
catalogs:
  v1:
    description: "Test"
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_none());
    }

    #[test]
    fn test_find_position_envs_at_start() {
        let content = r#"environments:
  sandbox:
    deployment: Sandbox
"#;
        let pos = find_environments_insert_position(content);
        assert!(pos.is_some());
    }

    // ========================================================================
    // environment_exists tests
    // ========================================================================

    #[test]
    fn test_env_exists_true() {
        let content = r#"
environments:
  sandbox:
    deployment: Sandbox
  cloud:
    deployment: CloudHosted
"#;
        assert!(environment_exists(content, "sandbox"));
        assert!(environment_exists(content, "cloud"));
    }

    #[test]
    fn test_env_exists_false() {
        let content = r#"
environments:
  sandbox:
    deployment: Sandbox
"#;
        assert!(!environment_exists(content, "cloud"));
        assert!(!environment_exists(content, "production"));
    }

    #[test]
    fn test_env_exists_similar_names() {
        let content = r#"
environments:
  sandbox:
    deployment: Sandbox
"#;
        // "sand" should not match "sandbox"
        assert!(!environment_exists(content, "sand"));
        // "sandboxes" should not match "sandbox"
        assert!(!environment_exists(content, "sandboxes"));
    }

    // ========================================================================
    // has_environments_section tests
    // ========================================================================

    #[test]
    fn test_has_envs_section_true() {
        let content = "foo: bar\nenvironments:\n  sandbox: {}";
        assert!(has_environments_section(content));
    }

    #[test]
    fn test_has_envs_section_at_start() {
        let content = "environments:\n  sandbox: {}";
        assert!(has_environments_section(content));
    }

    #[test]
    fn test_has_envs_section_false() {
        let content = "catalogs:\n  v1: {}";
        assert!(!has_environments_section(content));
    }

    // ========================================================================
    // insert_environment tests
    // ========================================================================

    #[test]
    fn test_insert_new_environment() {
        let content = r#"---
catalogs:
  v1:
    description: "Test"

environments:
  sandbox:
    deployment: Sandbox
"#;
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: Some("Test".to_string()),
            workspace: Some("test-ws".to_string()),
            binding_address: None,
            port: None,
            facilitator: None,
            network: None,
        };

        let result = insert_environment(content, "cloud", &config);

        assert!(result.changed);
        assert!(result.content.contains("  cloud:"));
        assert!(result.content.contains("    deployment: CloudHosted"));
        assert!(result.content.contains("    workspace: test-ws"));
        // Original content preserved
        assert!(result.content.contains("catalogs:"));
        assert!(result.content.contains("sandbox:"));
    }

    #[test]
    fn test_insert_existing_environment_skipped() {
        let content = r#"
environments:
  cloud:
    deployment: CloudHosted
"#;
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: Some("New".to_string()),
            workspace: Some("new-ws".to_string()),
            binding_address: None,
            port: None,
            facilitator: None,
            network: None,
        };

        let result = insert_environment(content, "cloud", &config);

        assert!(!result.changed);
        assert!(result.message.contains("already exists"));
        // Content unchanged
        assert_eq!(result.content, content);
    }

    #[test]
    fn test_insert_creates_envs_section() {
        let content = r#"---
catalogs:
  v1:
    description: "Test"
"#;
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: None,
            workspace: Some("test".to_string()),
            binding_address: None,
            port: None,
            facilitator: None,
            network: None,
        };

        let result = insert_environment(content, "cloud", &config);

        assert!(result.changed);
        assert!(result.content.contains("environments:"));
        assert!(result.content.contains("  cloud:"));
        assert!(result.message.contains("Created environments section"));
    }

    #[test]
    fn test_insert_preserves_formatting() {
        let content = r#"---
# MoneyMQ Manifest
catalogs:
  v1:
    description: 'Surfpool'
    catalog_path: billing/v1

# Payment acceptance configuration
payments:
  networks:
    chain: Solana
    stablecoins:
      - USDC

# Deployment environments
environments:
  # Default settings for the local development environment
  # sandbox:
  #   deployment: Sandbox
"#;
        let config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: Some("My Project".to_string()),
            workspace: Some("my-ws".to_string()),
            binding_address: None,
            port: None,
            facilitator: Some(IacFacilitatorConfig {
                fee: Some(0),
                key_management: Some(KeyManagement::TurnKey),
            }),
            network: None,
        };

        let result = insert_environment(content, "cloud", &config);

        assert!(result.changed);

        // Original comments preserved
        assert!(result.content.contains("# MoneyMQ Manifest"));
        assert!(
            result
                .content
                .contains("# Payment acceptance configuration")
        );
        assert!(result.content.contains("# Deployment environments"));
        assert!(
            result
                .content
                .contains("# Default settings for the local development environment")
        );

        // Original structure preserved
        assert!(result.content.contains("catalogs:"));
        assert!(result.content.contains("payments:"));
        assert!(result.content.contains("stablecoins:"));
        assert!(result.content.contains("- USDC"));

        // New environment added
        assert!(result.content.contains("  cloud:"));
        assert!(result.content.contains("    deployment: CloudHosted"));
    }

    #[test]
    fn test_insert_multiple_environments() {
        let mut content = r#"---
environments:
"#
        .to_string();

        let cloud_config = IacEnvironmentConfig {
            deployment: DeploymentType::CloudHosted,
            project: None,
            workspace: Some("prod".to_string()),
            binding_address: None,
            port: None,
            facilitator: None,
            network: None,
        };

        let staging_config = IacEnvironmentConfig {
            deployment: DeploymentType::SelfHosted,
            project: None,
            workspace: None,
            binding_address: Some("0.0.0.0".to_string()),
            port: Some(8488),
            facilitator: None,
            network: None,
        };

        // Insert first
        let result = insert_environment(&content, "cloud", &cloud_config);
        assert!(result.changed);
        content = result.content;

        // Insert second
        let result = insert_environment(&content, "staging", &staging_config);
        assert!(result.changed);
        content = result.content;

        // Both should exist
        assert!(content.contains("  cloud:"));
        assert!(content.contains("  staging:"));
        assert!(content.contains("    deployment: CloudHosted"));
        assert!(content.contains("    deployment: SelfHosted"));
    }
}

// ============================================================================
// Product File Synchronization
// ============================================================================

/// Merge product updates into existing file content, preserving unmodified fields.
///
/// This function reads an existing product YAML file and merges in the provided
/// updates while preserving fields that aren't in the update (like deployed_id,
/// sandboxes, timestamps, etc.) and maintaining field order.
pub fn merge_product_update(existing_content: &str, update: &ProductSchema) -> SyncResult {
    // Parse existing content as a generic YAML value
    let mut existing: serde_yml::Value = match serde_yml::from_str(existing_content) {
        Ok(v) => v,
        Err(_) => {
            // If parsing fails, return the update as-is
            return SyncResult {
                content: existing_content.to_string(),
                changed: false,
                message: "Failed to parse existing content".to_string(),
            };
        }
    };

    let Some(existing_map) = existing.as_mapping_mut() else {
        return SyncResult {
            content: existing_content.to_string(),
            changed: false,
            message: "Existing content is not a mapping".to_string(),
        };
    };

    // Helper to update a field in place or insert at end
    // Uses shift_remove + insert to avoid duplicate key issues
    fn update_field(map: &mut serde_yml::Mapping, key: &str, value: serde_yml::Value) {
        let key_val = serde_yml::Value::String(key.to_string());
        // Remove existing key first to avoid duplicates, then insert
        map.shift_remove(&key_val);
        map.insert(key_val, value);
    }

    // Helper to remove a field entirely
    fn remove_field(map: &mut serde_yml::Mapping, key: &str) {
        let key_val = serde_yml::Value::String(key.to_string());
        map.shift_remove(&key_val);
    }

    // Update name (required field)
    update_field(
        existing_map,
        "name",
        serde_yml::Value::String(update.name.clone()),
    );

    // Note: We do NOT write 'id' to the file - it's computed from the file path

    // Optional fields - empty/null values remove the field from YAML
    match &update.description {
        Some(desc) if !desc.is_empty() => {
            update_field(
                existing_map,
                "description",
                serde_yml::Value::String(desc.clone()),
            );
        }
        Some(_) => {
            // Empty string means remove the field
            remove_field(existing_map, "description");
        }
        None => {
            // None means don't touch the field (leave as-is)
        }
    }

    if let Some(active) = update.active {
        update_field(existing_map, "active", serde_yml::Value::Bool(active));
    }

    // Optional string fields - empty means remove
    match &update.product_type {
        Some(v) if !v.is_empty() => {
            update_field(
                existing_map,
                "product_type",
                serde_yml::Value::String(v.clone()),
            );
        }
        Some(_) => remove_field(existing_map, "product_type"),
        None => {}
    }

    match &update.unit_label {
        Some(v) if !v.is_empty() => {
            update_field(
                existing_map,
                "unit_label",
                serde_yml::Value::String(v.clone()),
            );
        }
        Some(_) => remove_field(existing_map, "unit_label"),
        None => {}
    }

    match &update.statement_descriptor {
        Some(v) if !v.is_empty() => {
            update_field(
                existing_map,
                "statement_descriptor",
                serde_yml::Value::String(v.clone()),
            );
        }
        Some(_) => remove_field(existing_map, "statement_descriptor"),
        None => {}
    }

    // Images - empty array means remove
    match &update.images {
        Some(imgs) if !imgs.is_empty() => {
            if let Ok(images_value) = serde_yml::to_value(imgs) {
                update_field(existing_map, "images", images_value);
            }
        }
        Some(_) => remove_field(existing_map, "images"),
        None => {}
    }

    // Metadata - empty map means remove
    match &update.metadata {
        Some(meta) if !meta.is_empty() => {
            if let Ok(metadata_value) = serde_yml::to_value(meta) {
                update_field(existing_map, "metadata", metadata_value);
            }
        }
        Some(_) => remove_field(existing_map, "metadata"),
        None => {}
    }

    // Update price (singular) - preserve extra fields from existing price if present
    if let Some(price) = &update.price {
        if price.amounts.is_empty() {
            // Remove price if empty amounts
            existing_map.remove(serde_yml::Value::String("price".to_string()));
        } else if let Ok(update_price_val) = serde_yml::to_value(price) {
            // Try to merge with existing price structure
            let existing_price = existing_map
                .get(serde_yml::Value::String("price".to_string()))
                .and_then(|v| v.as_mapping())
                .cloned();

            if let Some(mut existing_price_map) = existing_price {
                // Merge update values into existing
                if let Some(update_map) = update_price_val.as_mapping() {
                    for (key, value) in update_map.iter() {
                        if let Some(existing_val) = existing_price_map.get_mut(key) {
                            *existing_val = value.clone();
                        } else {
                            existing_price_map.insert(key.clone(), value.clone());
                        }
                    }
                }
                existing_map.insert(
                    serde_yml::Value::String("price".to_string()),
                    serde_yml::Value::Mapping(existing_price_map),
                );
            } else {
                // No existing price, just set the new one
                update_field(existing_map, "price", update_price_val);
            }
        }
    }

    // Remove internal fields that should not be persisted to YAML
    // These are computed at runtime from file paths
    existing_map.remove(serde_yml::Value::String("id".to_string()));
    existing_map.remove(serde_yml::Value::String("_source_file".to_string()));
    existing_map.remove(serde_yml::Value::String("_product_dir".to_string()));
    existing_map.remove(serde_yml::Value::String("_variant".to_string()));

    // Generate the final YAML
    match crate::yaml_util::to_pretty_yaml_with_header(&existing, Some("Product"), Some("v1")) {
        Ok(content) => SyncResult {
            content,
            changed: true,
            message: format!("Updated product '{}'", update.id),
        },
        Err(e) => SyncResult {
            content: existing_content.to_string(),
            changed: false,
            message: format!("Failed to serialize: {}", e),
        },
    }
}

#[cfg(test)]
mod product_tests {
    use indexmap::IndexMap;

    use super::*;

    fn make_product(id: &str, name: &str) -> ProductSchema {
        ProductSchema {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            active: None,
            product_type: None,
            statement_descriptor: None,
            unit_label: None,
            images: None,
            metadata: None,
            features: None,
            price: None,
            _source_file: None,
            _product_dir: None,
            _variant: None,
        }
    }

    /// Create a price schema with the given amount in dollars
    fn make_price(amount: f64) -> super::super::PriceSchema {
        let mut amounts = IndexMap::new();
        amounts.insert("usd".to_string(), amount);
        super::super::PriceSchema {
            id: None,
            amounts,
            pricing_type: None,
            recurring: None,
            overage: None,
            trial: None,
            active: Some(true),
            nickname: None,
            metadata: None,
        }
    }

    #[test]
    fn test_merge_preserves_deployed_id() {
        let existing = r#"---
id: prod_123
name: Old Name
deployed_id: prod_stripe_abc
sandboxes:
  default: prod_test_xyz
"#;
        let update = make_product("prod_123", "New Name");

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        assert!(result.content.contains("name: New Name"));
        assert!(result.content.contains("deployed_id: prod_stripe_abc"));
        assert!(result.content.contains("default: prod_test_xyz"));
    }

    #[test]
    fn test_merge_preserves_timestamps() {
        let existing = r#"---
id: prod_123
name: Test Product
created_at: 1704067200
updated_at: 1718451000
"#;
        let mut update = make_product("prod_123", "Updated Product");
        update.description = Some("New description".to_string());

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        assert!(result.content.contains("name: Updated Product"));
        assert!(result.content.contains("description: New description"));
        // Timestamps should be preserved (format may vary slightly)
        assert!(result.content.contains("created_at:"));
        assert!(result.content.contains("updated_at:"));
    }

    #[test]
    fn test_merge_updates_metadata() {
        let existing = r#"---
id: prod_123
name: Test Product
metadata:
  old_key: old_value
  preserved_key: should_stay
"#;
        let mut update = make_product("prod_123", "Test Product");
        let mut metadata = IndexMap::new();
        metadata.insert("new_key".to_string(), serde_json::json!("new_value"));
        update.metadata = Some(metadata);

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // New metadata replaces old
        assert!(result.content.contains("new_key: new_value"));
    }

    #[test]
    fn test_merge_preserves_price_extra_fields() {
        let existing = r#"---
id: prod_123
name: Test Product
price:
  amounts:
    usd: 10.00
  deployed_id: price_stripe_abc
  sandboxes:
    default: price_test_xyz
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.price = Some(make_price(10.00));

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Extra fields from existing price should be preserved
        assert!(result.content.contains("deployed_id: price_stripe_abc"));
        assert!(result.content.contains("default: price_test_xyz"));
    }

    #[test]
    fn test_merge_updates_price_amount() {
        let existing = r#"---
id: prod_123
name: Test Product
price:
  amounts:
    usd: 10.00
  deployed_id: price_existing
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.price = Some(make_price(20.00)); // Update amount

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Existing price keeps its deployed_id
        assert!(result.content.contains("deployed_id: price_existing"));
        // Amount is updated
        assert!(result.content.contains("usd: 20"));
    }

    #[test]
    fn test_merge_removes_internal_fields() {
        // Internal fields (id, _source_file, _product_dir, _variant) should never be written to YAML
        // These are computed at runtime from file paths
        let existing = r#"---
id: prod_123
name: Test Product
_source_file: surfboard
_product_dir: surfnet
_variant: light
"#;
        let update = make_product("prod_123", "Updated Name");

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // All internal fields should be removed
        assert!(
            !result.content.contains("_source_file"),
            "_source_file should not be in output"
        );
        assert!(
            !result.content.contains("_product_dir"),
            "_product_dir should not be in output"
        );
        assert!(
            !result.content.contains("_variant"),
            "_variant should not be in output"
        );
        // id is computed from file path, not stored
        assert!(
            !result.content.contains("\nid:"),
            "id should not be in output"
        );
    }

    #[test]
    fn test_merge_does_not_write_empty_prices() {
        // When prices is empty, don't write "prices: []" to the file
        let existing = r#"---
name: Base Product
product_type: service
"#;
        let mut update = make_product("prod_123", "Base Product");
        update.price = None; // No price

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Should not write price field when None
        assert!(
            !result.content.contains("price:"),
            "None price should not be written"
        );
    }

    #[test]
    fn test_merge_empty_description_removes_field() {
        // When description is set to empty string, the field should be removed from YAML
        let existing = r#"---
name: Test Product
description: This is a description
product_type: service
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.description = Some("".to_string()); // Empty description

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Description field should be removed entirely
        assert!(
            !result.content.contains("description:"),
            "empty description should remove the field"
        );
        // Other fields should still be present
        assert!(result.content.contains("name: Test Product"));
        assert!(result.content.contains("product_type: service"));
    }

    #[test]
    fn test_merge_updates_active_status() {
        let existing = r#"---
id: prod_123
name: Test Product
active: true
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.active = Some(false);

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        assert!(result.content.contains("active: false"));
    }

    #[test]
    fn test_merge_invalid_yaml_returns_unchanged() {
        let existing = "this is not valid yaml: [[[";
        let update = make_product("prod_123", "Test");

        let result = merge_product_update(existing, &update);

        assert!(!result.changed);
        assert!(result.message.contains("Failed to parse"));
    }

    #[test]
    fn test_merge_preserves_unknown_fields() {
        let existing = r#"---
id: prod_123
name: Test Product
custom_field: some_value
another_custom: 123
nested:
  deep: value
"#;
        let update = make_product("prod_123", "Updated Name");

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        assert!(result.content.contains("name: Updated Name"));
        assert!(result.content.contains("custom_field: some_value"));
        assert!(result.content.contains("another_custom: 123"));
        assert!(result.content.contains("deep: value"));
    }

    #[test]
    fn test_merge_preserves_price_field_order() {
        let existing = r#"---
id: prod_123
name: Test Product
price:
  id: price_abc
  deployed_id: price_stripe_123
  amounts:
    usd: 10.00
  nickname: Enterprise
  active: true
  metadata: {}
  created_at: 1704067200
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.price = Some(make_price(20.00)); // Changed amount

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Verify fields are preserved
        assert!(result.content.contains("deployed_id: price_stripe_123"));
        assert!(result.content.contains("nickname: Enterprise"));
        assert!(result.content.contains("created_at:"));
        // Verify updated field
        assert!(result.content.contains("usd: 20"));
        // Verify field order is roughly preserved (id should come before amounts)
        let id_pos = result.content.find("id: price_abc").unwrap();
        let amount_pos = result.content.find("usd: 20").unwrap();
        assert!(id_pos < amount_pos, "Field order not preserved");
    }

    #[test]
    fn test_merge_preserves_product_field_order() {
        let existing = r#"---
deployed_id: prod_stripe_abc
name: Test Product
description: Original description
active: true
created_at: 1704067200
updated_at: 1718451000
product_type: service
"#;
        let mut update = make_product("prod_123", "Updated Name");
        update.description = Some("New description".to_string());

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Verify id is NOT written to file (it's computed from file path)
        assert!(
            !result.content.contains("\nid:"),
            "id should not be written to file"
        );
        // Verify field order: deployed_id should come before name, name before description
        let deployed_pos = result.content.find("deployed_id: prod_stripe_abc").unwrap();
        let name_pos = result.content.find("name: Updated Name").unwrap();
        let desc_pos = result.content.find("description: New description").unwrap();
        assert!(
            deployed_pos < name_pos,
            "deployed_id should come before name"
        );
        assert!(name_pos < desc_pos, "name should come before description");
    }

    #[test]
    fn test_merge_price_with_recurring_config() {
        let existing = r#"---
id: prod_123
name: Test Product
price:
  amounts:
    usd: 10.00
  deployed_id: price_stripe_123
  created_at: 1704067200
"#;
        let mut update = make_product("prod_123", "Test Product");
        // Add recurring config to price
        let mut amounts = IndexMap::new();
        amounts.insert("usd".to_string(), 10.0);
        update.price = Some(super::super::PriceSchema {
            id: None,
            amounts,
            pricing_type: Some(super::super::PricingType::Recurring),
            recurring: Some(super::super::RecurringConfig {
                interval: super::super::RecurringInterval::Month,
                interval_count: None,
            }),
            overage: None,
            trial: None,
            active: Some(true),
            nickname: None,
            metadata: None,
        });

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Should be recurring now
        assert!(result.content.contains("pricing_type: recurring"));
        // Should preserve deployed_id and created_at
        assert!(result.content.contains("deployed_id: price_stripe_123"));
        assert!(result.content.contains("created_at:"));
    }

    #[test]
    fn test_merge_preserves_price_deployed_id() {
        let existing = r#"---
id: prod_123
name: Test Product
price:
  deployed_id: price_stripe_123
  amounts:
    usd: 10.00
  created_at: 1704067200
"#;
        let mut update = make_product("prod_123", "Test Product");
        update.price = Some(make_price(20.00)); // Changed amount

        let result = merge_product_update(existing, &update);

        assert!(result.changed);
        // Should preserve deployed_id
        assert!(result.content.contains("deployed_id: price_stripe_123"));
        // Should have updated amount
        assert!(result.content.contains("usd: 20"));
    }
}
