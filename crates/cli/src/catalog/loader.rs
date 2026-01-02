//! Catalog loader for variant-based product definitions
//!
//! Supports two product formats:
//! 1. Legacy flat files: `products/{id}.yaml` - single product per file
//! 2. Variant-based: `products/{product}/product.yaml` + variants
//!
//! Variant layouts supported:
//! - Old: `products/{product}/variants/{variant}.yaml`
//! - New: `products/{product}/variants/{variant}/product.yaml`
//!
//! In variant-based format:
//! - `product.yaml` defines base fields (product_type, unit_label, metadata schema, price defaults)
//! - Each variant overrides specific fields via deep merge
//! - Product ID is generated from `{product_dir}-{variant_name}` if not specified

use std::{collections::HashMap, fs, path::Path};

use console::style;
use moneymq_types::{
    Currency, Price, PricingType, Product, ProductFeature, merge_product_with_variant,
};
use serde_json::Value as JsonValue;

/// Load all products from a catalog directory
///
/// Supports both legacy flat files and variant-based directories
pub fn load_products_from_directory(
    catalog_dir: &Path,
) -> Result<HashMap<String, Product>, String> {
    let mut products = HashMap::new();

    if !catalog_dir.exists() {
        return Ok(products);
    }

    let entries = fs::read_dir(catalog_dir)
        .map_err(|e| format!("Failed to read catalog directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            // Check for variant-based product directory
            let product_yaml = path.join("product.yaml");
            if product_yaml.exists() {
                let dir_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                match load_product_with_variants(&path, dir_name) {
                    Ok(loaded_products) => {
                        for product in loaded_products {
                            products.insert(product.id.clone(), product);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to parse product directory {}: {}",
                            style("Warning:").yellow(),
                            path.display(),
                            e
                        );
                    }
                }
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            // Legacy flat file format
            match load_legacy_product(&path) {
                Ok(product) => {
                    products.insert(product.id.clone(), product);
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to parse {}: {}",
                        style("Warning:").yellow(),
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(products)
}

/// Load a legacy flat product YAML file
fn load_legacy_product(path: &Path) -> Result<Product, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    serde_yml::from_str::<Product>(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Load a product directory with variants (supports recursive/nested variants)
fn load_product_with_variants(product_dir: &Path, dir_name: &str) -> Result<Vec<Product>, String> {
    // Load base product.yaml content
    let product_yaml_path = product_dir.join("product.yaml");
    let base_content = fs::read_to_string(&product_yaml_path)
        .map_err(|e| format!("Failed to read product.yaml: {}", e))?;

    // Load variants from variants directory
    let variants_dir = product_dir.join("variants");
    if !variants_dir.exists() {
        return Err("No variants directory found".to_string());
    }

    // Recursively load variants with an empty path prefix
    load_variants_recursive(&variants_dir, &base_content, dir_name, "")
}

/// Recursively load variants from a directory
/// `id_prefix` accumulates the path for nested variants (e.g., "lite" -> "lite-a")
fn load_variants_recursive(
    variants_dir: &Path,
    base_content: &str,
    product_dir_name: &str,
    id_prefix: &str,
) -> Result<Vec<Product>, String> {
    let mut products = Vec::new();

    let entries = fs::read_dir(variants_dir)
        .map_err(|e| format!("Failed to read variants directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read variant entry: {}", e))?;
        let path = entry.path();

        // Support both layouts:
        // Old: variants/{variant}.yaml (file)
        // New: variants/{variant}/product.yaml (directory)
        if path.is_dir() {
            // New layout: variants/{variant}/product.yaml
            let variant_yaml = path.join("product.yaml");
            if !variant_yaml.exists() {
                continue;
            }
            let variant_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // Build the full variant identifier (e.g., "lite" or "lite-a")
            let full_variant_id = if id_prefix.is_empty() {
                variant_name.to_string()
            } else {
                format!("{}-{}", id_prefix, variant_name)
            };

            // Check for nested variants
            let nested_variants_dir = path.join("variants");
            if nested_variants_dir.exists() {
                // This variant has its own variants - merge this level first, then recurse
                let variant_content = fs::read_to_string(&variant_yaml)
                    .map_err(|e| format!("Failed to read variant file: {}", e))?;

                // Merge base with this variant to create intermediate base
                let merged_base = moneymq_types::deep_merge_json(
                    serde_yml::from_str(base_content)
                        .map_err(|e| format!("Failed to parse base YAML: {}", e))?,
                    serde_yml::from_str(&variant_content)
                        .map_err(|e| format!("Failed to parse variant YAML: {}", e))?,
                );
                let merged_base_str = serde_json::to_string(&merged_base)
                    .map_err(|e| format!("Failed to serialize merged base: {}", e))?;

                // Recursively load nested variants with the merged base
                match load_variants_recursive(
                    &nested_variants_dir,
                    &merged_base_str,
                    product_dir_name,
                    &full_variant_id,
                ) {
                    Ok(nested_products) => {
                        products.extend(nested_products);
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to load nested variants in {}: {}",
                            style("Warning:").yellow(),
                            path.display(),
                            e
                        );
                    }
                }
            } else {
                // Leaf variant - no nested variants
                match load_and_merge_variant(
                    &variant_yaml,
                    base_content,
                    product_dir_name,
                    &full_variant_id,
                ) {
                    Ok(product) => {
                        products.push(product);
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to parse variant {}: {}",
                            style("Warning:").yellow(),
                            variant_yaml.display(),
                            e
                        );
                    }
                }
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            // Old layout: variants/{variant}.yaml
            let variant_name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let full_variant_id = if id_prefix.is_empty() {
                variant_name.to_string()
            } else {
                format!("{}-{}", id_prefix, variant_name)
            };

            match load_and_merge_variant(&path, base_content, product_dir_name, &full_variant_id) {
                Ok(product) => {
                    products.push(product);
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to parse variant {}: {}",
                        style("Warning:").yellow(),
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(products)
}

/// Load a variant file, deep merge with base, and convert to Product
fn load_and_merge_variant(
    variant_path: &Path,
    base_content: &str,
    product_dir_name: &str,
    variant_name: &str,
) -> Result<Product, String> {
    let variant_content = fs::read_to_string(variant_path)
        .map_err(|e| format!("Failed to read variant file: {}", e))?;

    // Deep merge base + variant using the shared utility
    let merged = merge_product_with_variant(
        base_content,
        &variant_content,
        product_dir_name,
        variant_name,
    )?;

    // Convert merged JSON to Product
    json_to_product(merged)
}

/// Convert merged JSON to a Product struct
pub fn json_to_product(json: JsonValue) -> Result<Product, String> {
    let obj = json.as_object().ok_or("Expected JSON object")?;

    // Extract ID (required - should be set by merge function)
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or("Missing product id")?;

    // Extract basic fields
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let active = obj.get("active").and_then(|v| v.as_bool()).unwrap_or(true);
    let product_type = obj
        .get("product_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let unit_label = obj
        .get("unit_label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let statement_descriptor = obj
        .get("statement_descriptor")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract images
    let images = obj
        .get("images")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Extract and convert metadata to the expected format
    let metadata = extract_metadata(obj.get("metadata"));

    // Extract and convert features
    let features = extract_features(obj.get("features"));

    // Extract and convert price (singular)
    let prices = extract_price(obj.get("price"))?;

    // Build Product using Product::new() for proper timestamps
    let mut product = Product::new()
        .with_some_name(name)
        .with_some_description(description)
        .with_some_product_type(product_type)
        .with_some_statement_descriptor(statement_descriptor)
        .with_some_unit_label(unit_label);

    product.id = id;
    product.active = active;
    product.images = images;
    product.metadata = metadata;
    product.features = features;
    product.prices = prices;

    Ok(product)
}

/// Extract metadata from JSON, converting to the expected IndexMap<String, String> format
fn extract_metadata(metadata_value: Option<&JsonValue>) -> indexmap::IndexMap<String, String> {
    let mut result = indexmap::IndexMap::new();

    if let Some(JsonValue::Object(metadata)) = metadata_value {
        for (key, value) in metadata {
            // Serialize the value as a JSON string
            let string_value = match value {
                JsonValue::String(s) => s.clone(),
                _ => serde_json::to_string(value).unwrap_or_default(),
            };
            result.insert(key.clone(), string_value);
        }
    }

    result
}

/// Extract features from JSON, converting to IndexMap<String, ProductFeature>
/// Features are deep merged: base product defines name/description, variants add values
fn extract_features(
    features_value: Option<&JsonValue>,
) -> indexmap::IndexMap<String, ProductFeature> {
    let mut result = indexmap::IndexMap::new();

    if let Some(JsonValue::Object(features)) = features_value {
        for (key, value) in features {
            if let JsonValue::Object(feature_obj) = value {
                let name = feature_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let description = feature_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let feature_value = feature_obj.get("value").cloned();

                result.insert(
                    key.clone(),
                    ProductFeature {
                        name,
                        description,
                        value: feature_value,
                    },
                );
            }
        }
    }

    result
}

/// Extract price from the merged JSON (singular price object)
/// New format uses `price: { amounts: { usd: 49.00 }, recurring: { interval: month } }`
fn extract_price(price_value: Option<&JsonValue>) -> Result<Vec<Price>, String> {
    // If price is present, convert to a single-element vector
    if let Some(JsonValue::Object(price_obj)) = price_value {
        let price = json_to_price(&JsonValue::Object(price_obj.clone()))?;
        Ok(vec![price])
    } else {
        Ok(vec![])
    }
}

/// Convert a price JSON object to a Price struct
/// New format: { amounts: { usd: 49.00 }, pricing_type: "recurring", recurring: { interval: "month" } }
fn json_to_price(json: &JsonValue) -> Result<Price, String> {
    let obj = json.as_object().ok_or("Expected price to be an object")?;

    // Extract amounts map - new format uses `amounts: { usd: 49.00 }`
    let (currency, unit_amount) = if let Some(JsonValue::Object(amounts)) = obj.get("amounts") {
        // Get the first currency/amount pair
        if let Some((currency_str, amount_value)) = amounts.iter().next() {
            let currency = Currency::parse(currency_str).unwrap_or(Currency::Usd);
            // Convert float dollars to integer cents
            let amount = amount_value.as_f64().map(|a| (a * 100.0).round() as i64);
            (currency, amount)
        } else {
            (Currency::Usd, None)
        }
    } else {
        // Fallback to legacy format for backwards compatibility
        let currency_str = obj
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("usd");
        let currency = Currency::parse(currency_str).unwrap_or(Currency::Usd);
        let amount = obj.get("unit_amount").and_then(|v| v.as_i64());
        (currency, amount)
    };

    // Extract pricing type (defaults to one_time if not specified)
    let pricing_type_str = obj
        .get("pricing_type")
        .and_then(|v| v.as_str())
        .unwrap_or("one_time");
    let pricing_type = match pricing_type_str {
        "recurring" => PricingType::Recurring,
        _ => PricingType::OneTime,
    };

    // Build price using Price::new() for proper timestamps
    let mut price = Price::new(currency, pricing_type).with_some_amount(unit_amount);

    // Extract optional fields
    if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
        price.id = id.to_string();
    }

    if let Some(active) = obj.get("active").and_then(|v| v.as_bool()) {
        price.active = active;
    }

    // Recurring config handling - new format uses nested `recurring: { interval: "month" }`
    if let Some(JsonValue::Object(recurring)) = obj.get("recurring") {
        if let Some(interval) = recurring.get("interval").and_then(|v| v.as_str()) {
            price.recurring_interval = match interval {
                "day" => Some(moneymq_types::RecurringInterval::Day),
                "week" => Some(moneymq_types::RecurringInterval::Week),
                "month" => Some(moneymq_types::RecurringInterval::Month),
                "year" => Some(moneymq_types::RecurringInterval::Year),
                _ => None,
            };
        }
        if let Some(count) = recurring.get("interval_count").and_then(|v| v.as_i64()) {
            price.recurring_interval_count = Some(count);
        }
    } else {
        // Fallback to legacy flat format for backwards compatibility
        if let Some(interval) = obj.get("recurring_interval").and_then(|v| v.as_str()) {
            price.recurring_interval = match interval {
                "day" => Some(moneymq_types::RecurringInterval::Day),
                "week" => Some(moneymq_types::RecurringInterval::Week),
                "month" => Some(moneymq_types::RecurringInterval::Month),
                "year" => Some(moneymq_types::RecurringInterval::Year),
                _ => None,
            };
        }
        if let Some(count) = obj.get("recurring_interval_count").and_then(|v| v.as_i64()) {
            price.recurring_interval_count = Some(count);
        }
    }

    Ok(price)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_id_generation_from_path() {
        let id = format!("{}-{}", "surfnet", "pro");
        assert_eq!(id, "surfnet-pro");
    }

    #[test]
    fn test_load_legacy_flat_product() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        fs::create_dir_all(&products_dir).unwrap();

        // Create a legacy flat product file
        let product_yaml = r#"---
id: prod_legacy123
name: Legacy Product
description: A legacy flat product file
active: true
metadata: {}
created_at: 1700000000
updated_at: null
product_type: service
images: []
prices:
  - id: price_legacy1
    active: true
    currency: usd
    unit_amount: 999
    pricing_type: one_time
    metadata: {}
    created_at: 1700000000
"#;
        fs::write(products_dir.join("legacy_product.yaml"), product_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        assert_eq!(products.len(), 1);
        let product = products.get("prod_legacy123").unwrap();
        assert_eq!(product.name, Some("Legacy Product".to_string()));
        assert_eq!(
            product.description,
            Some("A legacy flat product file".to_string())
        );
        assert!(product.active);
        assert_eq!(product.prices.len(), 1);
        assert_eq!(product.prices[0].unit_amount, Some(999));
    }

    #[test]
    fn test_load_variant_based_product_old_layout() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let surfnet_dir = products_dir.join("surfnet");
        let variants_dir = surfnet_dir.join("variants");
        fs::create_dir_all(&variants_dir).unwrap();

        // Create product.yaml (base)
        let product_yaml = r#"---
product_type: service
unit_label: per network
images: []
active: true
"#;
        fs::write(surfnet_dir.join("product.yaml"), product_yaml).unwrap();

        // Create light variant (old layout: direct .yaml file)
        let light_yaml = r#"---
name: Surfnet Starter
description: Perfect for testing and small projects.
price:
  amounts:
    usd: 3.99
"#;
        fs::write(variants_dir.join("light.yaml"), light_yaml).unwrap();

        // Create pro variant
        let pro_yaml = r#"---
name: Surfnet Pro
description: For active development.
price:
  amounts:
    usd: 9.98
"#;
        fs::write(variants_dir.join("pro.yaml"), pro_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        assert_eq!(products.len(), 2);

        // Check light variant
        let light = products.get("surfnet-light").unwrap();
        assert_eq!(light.name, Some("Surfnet Starter".to_string()));
        assert_eq!(light.product_type, Some("service".to_string()));
        assert_eq!(light.unit_label, Some("per network".to_string()));
        assert_eq!(light.prices.len(), 1);
        assert_eq!(light.prices[0].unit_amount, Some(399)); // 3.99 * 100

        // Check pro variant
        let pro = products.get("surfnet-pro").unwrap();
        assert_eq!(pro.name, Some("Surfnet Pro".to_string()));
        assert_eq!(pro.prices[0].unit_amount, Some(998)); // 9.98 * 100
    }

    #[test]
    fn test_load_variant_based_product_new_layout() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let surfnet_dir = products_dir.join("surfnet");
        let variants_dir = surfnet_dir.join("variants");

        // Create variant subdirectories (new layout)
        let light_dir = variants_dir.join("light");
        let pro_dir = variants_dir.join("pro");
        fs::create_dir_all(&light_dir).unwrap();
        fs::create_dir_all(&pro_dir).unwrap();

        // Create product.yaml (base)
        let product_yaml = r#"---
product_type: service
unit_label: per network
images: []
active: true
"#;
        fs::write(surfnet_dir.join("product.yaml"), product_yaml).unwrap();

        // Create light variant (new layout: variants/light/product.yaml)
        let light_yaml = r#"---
name: Surfnet Starter
description: Perfect for testing and small projects.
price:
  amounts:
    usd: 3.99
"#;
        fs::write(light_dir.join("product.yaml"), light_yaml).unwrap();

        // Create pro variant
        let pro_yaml = r#"---
name: Surfnet Pro
description: For active development.
price:
  amounts:
    usd: 9.99
"#;
        fs::write(pro_dir.join("product.yaml"), pro_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        assert_eq!(products.len(), 2);

        // Check light variant
        let light = products.get("surfnet-light").unwrap();
        assert_eq!(light.name, Some("Surfnet Starter".to_string()));
        assert_eq!(light.product_type, Some("service".to_string()));
        assert_eq!(light.unit_label, Some("per network".to_string()));
        assert_eq!(light.prices.len(), 1);
        assert_eq!(light.prices[0].unit_amount, Some(399)); // 3.99 * 100

        // Check pro variant
        let pro = products.get("surfnet-pro").unwrap();
        assert_eq!(pro.name, Some("Surfnet Pro".to_string()));
        assert_eq!(pro.prices[0].unit_amount, Some(999)); // 9.99 * 100
    }

    #[test]
    fn test_explicit_id_overrides_generated() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let myproduct_dir = products_dir.join("myproduct");
        let variants_dir = myproduct_dir.join("variants");
        fs::create_dir_all(&variants_dir).unwrap();

        // Create product.yaml
        let product_yaml = r#"---
product_type: service
active: true
"#;
        fs::write(myproduct_dir.join("product.yaml"), product_yaml).unwrap();

        // Create variant with explicit ID (no price - not a valid product)
        let variant_yaml = r#"---
id: custom_explicit_id
name: Custom Product
price:
  amounts:
    usd: 0
"#;
        fs::write(variants_dir.join("standard.yaml"), variant_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        assert_eq!(products.len(), 1);
        assert!(products.contains_key("custom_explicit_id"));
        assert!(!products.contains_key("myproduct-standard"));
    }

    #[test]
    fn test_deep_merge_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let surfnet_dir = products_dir.join("surfnet");
        let variants_dir = surfnet_dir.join("variants");
        fs::create_dir_all(&variants_dir).unwrap();

        // Create product.yaml with metadata schema
        let product_yaml = r#"---
product_type: service
metadata:
  transaction_limit:
    name: Transaction Limit
    description: Total transactions your network can process
  community_support:
    name: Community Support
    description: Access to community Discord and forums
"#;
        fs::write(surfnet_dir.join("product.yaml"), product_yaml).unwrap();

        // Create variant with metadata values
        let variant_yaml = r#"---
name: Surfnet Pro
metadata:
  transaction_limit:
    value: 500
  community_support:
    value: true
price:
  amounts:
    usd: 9.99
"#;
        fs::write(variants_dir.join("pro.yaml"), variant_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        let pro = products.get("surfnet-pro").unwrap();

        // Metadata should be deep merged - variant values override base
        assert!(pro.metadata.contains_key("transaction_limit"));
        assert!(pro.metadata.contains_key("community_support"));

        // The merged metadata should contain the variant's value field
        let tx_limit = &pro.metadata["transaction_limit"];
        assert!(tx_limit.contains("500") || tx_limit.contains("value"));
    }

    #[test]
    fn test_both_formats_coexist() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        fs::create_dir_all(&products_dir).unwrap();

        // Create a legacy flat file (with price)
        let legacy_yaml = r#"---
id: prod_legacy
name: Legacy Product
active: true
metadata: {}
created_at: 1700000000
updated_at: null
images: []
price:
  amounts:
    usd: 0
"#;
        fs::write(products_dir.join("legacy.yaml"), legacy_yaml).unwrap();

        // Create a variant-based product
        let variant_dir = products_dir.join("modern");
        let variants_dir = variant_dir.join("variants");
        fs::create_dir_all(&variants_dir).unwrap();

        let product_yaml = r#"---
product_type: service
active: true
"#;
        fs::write(variant_dir.join("product.yaml"), product_yaml).unwrap();

        let variant_yaml = r#"---
name: Modern Variant
price:
  amounts:
    usd: 0
"#;
        fs::write(variants_dir.join("basic.yaml"), variant_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        // Should have both products
        assert_eq!(products.len(), 2);
        assert!(products.contains_key("prod_legacy"));
        assert!(products.contains_key("modern-basic"));
    }

    #[test]
    fn test_recursive_variants() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let surfnet_dir = products_dir.join("surfnet");
        let variants_dir = surfnet_dir.join("variants");
        let lite_dir = variants_dir.join("lite");
        let lite_variants_dir = lite_dir.join("variants");
        let lite_a_dir = lite_variants_dir.join("a");
        let lite_b_dir = lite_variants_dir.join("b");

        fs::create_dir_all(&lite_a_dir).unwrap();
        fs::create_dir_all(&lite_b_dir).unwrap();

        // Base product
        let base_yaml = r#"---
product_type: service
unit_label: per network
active: true
features:
  transaction_limit:
    name: Transaction Limit
    description: Total transactions
"#;
        fs::write(surfnet_dir.join("product.yaml"), base_yaml).unwrap();

        // Lite variant (intermediate - has its own variants)
        let lite_yaml = r#"---
name: Surfnet Lite
description: Lite tier
"#;
        fs::write(lite_dir.join("product.yaml"), lite_yaml).unwrap();

        // Lite-A variant (leaf)
        let lite_a_yaml = r#"---
features:
  transaction_limit:
    value: 50
price:
  amounts:
    usd: 3.99
"#;
        fs::write(lite_a_dir.join("product.yaml"), lite_a_yaml).unwrap();

        // Lite-B variant (leaf)
        let lite_b_yaml = r#"---
features:
  transaction_limit:
    value: 100
price:
  amounts:
    usd: 5.99
"#;
        fs::write(lite_b_dir.join("product.yaml"), lite_b_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        // Should have 2 products: surfnet-lite-a and surfnet-lite-b
        assert_eq!(products.len(), 2);
        assert!(products.contains_key("surfnet-lite-a"));
        assert!(products.contains_key("surfnet-lite-b"));

        // Check lite-a
        let lite_a = products.get("surfnet-lite-a").unwrap();
        assert_eq!(lite_a.name, Some("Surfnet Lite".to_string())); // inherited from lite
        assert_eq!(lite_a.product_type, Some("service".to_string())); // inherited from base
        assert_eq!(lite_a.unit_label, Some("per network".to_string())); // inherited from base
        assert_eq!(lite_a.prices[0].unit_amount, Some(399)); // 3.99 * 100

        // Check features are merged correctly
        assert!(lite_a.features.contains_key("transaction_limit"));
        let tx_limit = &lite_a.features["transaction_limit"];
        assert_eq!(tx_limit.name, Some("Transaction Limit".to_string())); // from base
        assert_eq!(tx_limit.value, Some(serde_json::json!(50))); // from lite-a

        // Check lite-b
        let lite_b = products.get("surfnet-lite-b").unwrap();
        assert_eq!(lite_b.prices[0].unit_amount, Some(599)); // 5.99 * 100
        let tx_limit_b = &lite_b.features["transaction_limit"];
        assert_eq!(tx_limit_b.value, Some(serde_json::json!(100))); // from lite-b
    }

    #[test]
    fn test_features_deep_merge() {
        let temp_dir = TempDir::new().unwrap();
        let products_dir = temp_dir.path().join("products");
        let myproduct_dir = products_dir.join("myproduct");
        let variants_dir = myproduct_dir.join("variants");
        let pro_dir = variants_dir.join("pro");

        fs::create_dir_all(&pro_dir).unwrap();

        // Base product with feature definitions
        let base_yaml = r#"---
product_type: service
features:
  community_support:
    name: Community Support
    description: Access to community Discord
    value: true
  dedicated_support:
    name: Dedicated Support
    description: Direct access to team
  transaction_limit:
    name: Transaction Limit
    description: Total transactions allowed
"#;
        fs::write(myproduct_dir.join("product.yaml"), base_yaml).unwrap();

        // Pro variant with feature values
        let pro_yaml = r#"---
name: Pro Plan
features:
  dedicated_support:
    value: true
  transaction_limit:
    value: 500
price:
  amounts:
    usd: 9.99
"#;
        fs::write(pro_dir.join("product.yaml"), pro_yaml).unwrap();

        let products = load_products_from_directory(&products_dir).unwrap();

        assert_eq!(products.len(), 1);
        let pro = products.get("myproduct-pro").unwrap();

        // Check all features are present
        assert_eq!(pro.features.len(), 3);

        // community_support: should have both definition and value from base
        let community = &pro.features["community_support"];
        assert_eq!(community.name, Some("Community Support".to_string()));
        assert_eq!(community.value, Some(serde_json::json!(true)));

        // dedicated_support: should have definition from base, value from variant
        let dedicated = &pro.features["dedicated_support"];
        assert_eq!(dedicated.name, Some("Dedicated Support".to_string()));
        assert_eq!(dedicated.value, Some(serde_json::json!(true)));

        // transaction_limit: should have definition from base, value from variant
        let tx_limit = &pro.features["transaction_limit"];
        assert_eq!(tx_limit.name, Some("Transaction Limit".to_string()));
        assert_eq!(tx_limit.value, Some(serde_json::json!(500)));
    }
}
