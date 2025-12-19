/// Helper functions for YAML serialization with pretty formatting
use serde::Serialize;

/// Serialize a value to a pretty-formatted YAML string with Kubernetes-style header
///
/// # Arguments
/// * `value` - The value to serialize
/// * `resource_type` - Optional resource type (e.g., "Product", "Meter")
/// * `api_version` - Optional API version (defaults to "v1")
pub fn to_pretty_yaml_with_header<T: Serialize>(
    value: &T,
    resource_type: Option<&str>,
    api_version: Option<&str>,
) -> Result<String, String> {
    to_pretty_yaml_with_header_and_footer(value, resource_type, api_version, None)
}

/// Serialize a value to a pretty-formatted YAML string with header and optional footer
///
/// # Arguments
/// * `value` - The value to serialize
/// * `resource_type` - Optional resource type (e.g., "Product", "Meter", "Manifest")
/// * `api_version` - Optional API version (defaults to "v1")
/// * `footer` - Optional footer text to append (e.g., commented configuration examples)
pub fn to_pretty_yaml_with_header_and_footer<T: Serialize>(
    value: &T,
    resource_type: Option<&str>,
    api_version: Option<&str>,
    footer: Option<&str>,
) -> Result<String, String> {
    // First serialize with serde_yml
    let yaml_str =
        serde_yml::to_string(value).map_err(|e| format!("Failed to serialize to YAML: {}", e))?;

    // Then format with pretty_yaml for better indentation
    let options = pretty_yaml::config::FormatOptions::default();
    let formatted = pretty_yaml::format_text(&yaml_str, &options)
        .map_err(|e| format!("Failed to format YAML: {}", e))?;

    // Add Kubernetes-style header
    let version = api_version.unwrap_or("v1");
    let mut output = String::new();
    output.push_str("---\n");

    if let Some(rtype) = resource_type {
        output.push_str(&format!("# MoneyMQ {} - API version {}\n", rtype, version));
    } else {
        output.push_str(&format!("# MoneyMQ - API version {}\n", version));
    }

    output.push_str(&formatted);

    // Add footer if provided
    if let Some(footer_text) = footer {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(footer_text);
    }

    Ok(output)
}
