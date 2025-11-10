use std::{
    fs,
    path::{Path, PathBuf},
};

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
            r#"---
# MoneyMQ Manifest - API version v1
catalogs:
  {}:
    description: "{}'s catalog"
    catalog_path: {}
"#,
            catalog_version, dir_name, catalog_path
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
