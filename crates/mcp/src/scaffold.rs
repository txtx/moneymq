use std::{
    fs,
    path::{Path, PathBuf},
};

use moneymq_types::x402::config::constants::DEFAULT_PAYMENTS_FOOTER;

/// Scaffold a MoneyMQ project with directory structure and manifest file
///
/// Creates:
/// - moneymq.yaml manifest file (if it doesn't exist)
/// - billing/{catalog_version}/products/ directory
/// - billing/{catalog_version}/meters/ directory
/// - billing/{catalog_version}/assets/ directory
///
/// # Arguments
/// * `project_path` - Root path where project should be scaffolded
/// * `provider_name` - Provider name (e.g., "stripe")
/// * `catalog_version` - Version/account identifier for catalog path (e.g., "v1" or "acme_corp")
///
/// # Returns
/// Tuple of (products_path, meters_path, assets_path)
pub fn scaffold_moneymq_project(
    project_path: &Path,
    _provider_name: &str,
    catalog_version: &str,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let catalog_path = format!("billing/{}", catalog_version);

    // Create moneymq.yaml if it doesn't exist
    let manifest_path = project_path.join(moneymq_types::MANIFEST_FILE_NAME);
    if !manifest_path.exists() {
        // Get directory name for description - canonicalize to handle relative paths like "."
        let dir_name = project_path
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "project".to_string());

        let manifest_content = format!(
            "---\n# MoneyMQ Manifest - API version v1\ncatalogs:\n  {}:\n    description: \"{}'s catalog\"\n    catalog_path: {}\n\n{}",
            catalog_version, dir_name, catalog_path, DEFAULT_PAYMENTS_FOOTER
        );

        fs::write(&manifest_path, manifest_content)
            .map_err(|e| format!("Failed to write manifest file: {}", e))?;
    }

    // Create directory structure
    let products_path = project_path.join(&catalog_path).join("products");
    let meters_path = project_path.join(&catalog_path).join("meters");
    let assets_path = project_path.join(&catalog_path).join("assets");

    fs::create_dir_all(&products_path)
        .map_err(|e| format!("Failed to create products directory: {}", e))?;

    fs::create_dir_all(&meters_path)
        .map_err(|e| format!("Failed to create meters directory: {}", e))?;

    fs::create_dir_all(&assets_path)
        .map_err(|e| format!("Failed to create assets directory: {}", e))?;

    Ok((products_path, meters_path, assets_path))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_scaffold_includes_payments_footer() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let project_path = temp_dir.path();

        // Scaffold a new project
        scaffold_moneymq_project(project_path, "stripe", "v1").expect("Failed to scaffold project");

        // Read the generated manifest
        let manifest_path = project_path.join("moneymq.yaml");
        assert!(manifest_path.exists(), "Manifest file should exist");

        let content = fs::read_to_string(&manifest_path).expect("Failed to read manifest");

        // Verify header
        assert!(content.starts_with("---\n"));
        assert!(content.contains("# MoneyMQ Manifest - API version v1"));

        // Verify catalog section
        assert!(content.contains("catalogs:"));
        assert!(content.contains("v1:"));

        // Verify payments footer is present
        assert!(content.contains("# Payment configuration for accepting crypto payments"));
        assert!(content.contains("# payments:"));
        assert!(content.contains("#   stablecoins:"));
        assert!(content.contains("#     protocol: x402"));
        assert!(content.contains("# Learn more: https://docs.moneymq.co/payments"));
        assert!(content.contains("service_url: https://facilitator.moneymq.co"));
        assert!(content.contains("binding_address: 0.0.0.0"));
        assert!(content.contains("rpc_binding_port: 8899"));

        println!("Generated manifest:\n{}", content);
    }
}
