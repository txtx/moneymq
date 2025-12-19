//! Linting system for MoneyMQ manifests and catalogs.
//!
//! Provides validation rules to ensure configuration correctness before deployment.
//!
//! # Endpoints
//!
//! - `GET /iac/lint/diagnostics` - Get all lint diagnostics for the current configuration

use std::path::Path;

use clap::Parser;
use console::style;
use serde::{Deserialize, Serialize};

use crate::Context;

// ============================================================================
// Diagnostic Types
// ============================================================================

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Critical issues that prevent deployment
    Error,
    /// Issues that should be addressed but don't block deployment
    Warning,
    /// Informational messages and suggestions
    Info,
}

/// A single diagnostic message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Unique rule identifier (e.g., "product-requires-price")
    pub rule: String,

    /// Human-readable message
    pub message: String,

    /// Severity level
    pub severity: DiagnosticSeverity,

    /// File path where the issue was found (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    /// Line number in the file (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,

    /// Related entity ID (e.g., product ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,

    /// Related entity type (e.g., "product", "price", "meter")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

impl Diagnostic {
    /// Create a new error diagnostic
    pub fn error(rule: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: DiagnosticSeverity::Error,
            file: None,
            line: None,
            entity_id: None,
            entity_type: None,
        }
    }

    /// Create a new warning diagnostic
    pub fn warning(rule: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: DiagnosticSeverity::Warning,
            file: None,
            line: None,
            entity_id: None,
            entity_type: None,
        }
    }

    /// Create a new info diagnostic
    pub fn info(rule: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            message: message.into(),
            severity: DiagnosticSeverity::Info,
            file: None,
            line: None,
            entity_id: None,
            entity_type: None,
        }
    }

    /// Set the file path
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Set the line number
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// Set the entity information
    pub fn with_entity(
        mut self,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
    ) -> Self {
        self.entity_type = Some(entity_type.into());
        self.entity_id = Some(entity_id.into());
        self
    }
}

/// Result of running all lint rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResult {
    /// All diagnostics found
    pub diagnostics: Vec<Diagnostic>,

    /// Number of errors
    pub error_count: usize,

    /// Number of warnings
    pub warning_count: usize,

    /// Number of info messages
    pub info_count: usize,

    /// Whether the configuration is valid (no errors)
    pub is_valid: bool,
}

impl LintResult {
    /// Create from a list of diagnostics
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count();
        let info_count = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Info)
            .count();

        Self {
            diagnostics,
            error_count,
            warning_count,
            info_count,
            is_valid: error_count == 0,
        }
    }
}

// ============================================================================
// Lint Rules
// ============================================================================

/// Run all lint rules on the manifest and catalog
pub fn lint_all(manifest_dir: &Path, catalog_path: &str) -> LintResult {
    let mut diagnostics = Vec::new();

    // Load and lint products
    let products_dir = manifest_dir.join(catalog_path).join("products");
    if products_dir.exists() {
        diagnostics.extend(lint_products(&products_dir));
    } else {
        diagnostics.push(
            Diagnostic::warning(
                "catalog-no-products-dir",
                format!("Products directory not found: {}/products", catalog_path),
            )
            .with_file(format!("{}/products", catalog_path)),
        );
    }

    LintResult::from_diagnostics(diagnostics)
}

/// Lint all products in the products directory
fn lint_products(products_dir: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let entries = match std::fs::read_dir(products_dir) {
        Ok(entries) => entries,
        Err(e) => {
            diagnostics.push(Diagnostic::error(
                "catalog-read-error",
                format!("Failed to read products directory: {}", e),
            ));
            return diagnostics;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error(
                        "product-read-error",
                        format!("Failed to read product file: {}", e),
                    )
                    .with_file(file_name.to_string()),
                );
                continue;
            }
        };

        // Parse product YAML
        let product: serde_yml::Value = match serde_yml::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error("product-parse-error", format!("Invalid YAML: {}", e))
                        .with_file(file_name.to_string()),
                );
                continue;
            }
        };

        // Get product ID for better error messages
        let product_id = product
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let product_name = product
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed Product");

        // Rule: product-requires-id
        if product.get("id").is_none() {
            diagnostics.push(
                Diagnostic::error(
                    "product-requires-id",
                    format!("Product '{}' is missing required 'id' field", product_name),
                )
                .with_file(file_name.to_string())
                .with_entity("product", product_id),
            );
        }

        // Rule: product-requires-name
        if product.get("name").is_none() {
            diagnostics.push(
                Diagnostic::error(
                    "product-requires-name",
                    format!("Product '{}' is missing required 'name' field", product_id),
                )
                .with_file(file_name.to_string())
                .with_entity("product", product_id),
            );
        }

        // Rule: product-requires-price
        let prices = product.get("prices");
        let has_prices = prices
            .map(|p| p.as_sequence().map(|s| !s.is_empty()).unwrap_or(false))
            .unwrap_or(false);

        if !has_prices {
            diagnostics.push(
                Diagnostic::error(
                    "product-requires-price",
                    format!(
                        "Product '{}' ({}) has no prices. All products must have at least one price.",
                        product_name, product_id
                    ),
                )
                .with_file(file_name.to_string())
                .with_entity("product", product_id),
            );
        }

        // Rule: price-requires-currency (for each price)
        if let Some(prices) = prices.and_then(|p| p.as_sequence()) {
            for (idx, price) in prices.iter().enumerate() {
                let price_id = price
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("price[{}]", idx));

                // Check currency
                if price.get("currency").is_none() {
                    diagnostics.push(
                        Diagnostic::error(
                            "price-requires-currency",
                            format!(
                                "Price '{}' on product '{}' is missing required 'currency' field",
                                price_id, product_name
                            ),
                        )
                        .with_file(file_name.to_string())
                        .with_entity("price", &price_id),
                    );
                }

                // Check unit_amount
                if price.get("unit_amount").is_none() {
                    diagnostics.push(
                        Diagnostic::error(
                            "price-requires-unit-amount",
                            format!(
                                "Price '{}' on product '{}' is missing required 'unit_amount' field",
                                price_id, product_name
                            ),
                        )
                        .with_file(file_name.to_string())
                        .with_entity("price", &price_id),
                    );
                }

                // Check pricing_type
                if price.get("pricing_type").is_none() {
                    diagnostics.push(
                        Diagnostic::warning(
                            "price-missing-pricing-type",
                            format!(
                                "Price '{}' on product '{}' is missing 'pricing_type' (defaulting to 'one_time')",
                                price_id, product_name
                            ),
                        )
                        .with_file(file_name.to_string())
                        .with_entity("price", &price_id),
                    );
                }

                // Check recurring prices have interval
                if let Some(pricing_type) = price.get("pricing_type").and_then(|v| v.as_str()) {
                    if pricing_type == "recurring" && price.get("interval").is_none() {
                        diagnostics.push(
                            Diagnostic::error(
                                "recurring-price-requires-interval",
                                format!(
                                    "Recurring price '{}' on product '{}' is missing required 'interval' field",
                                    price_id, product_name
                                ),
                            )
                            .with_file(file_name.to_string())
                            .with_entity("price", &price_id),
                        );
                    }
                }
            }
        }

        // Rule: product-description-recommended (info)
        if product.get("description").is_none() {
            diagnostics.push(
                Diagnostic::info(
                    "product-description-recommended",
                    format!(
                        "Product '{}' ({}) has no description. Consider adding one for better clarity.",
                        product_name, product_id
                    ),
                )
                .with_file(file_name.to_string())
                .with_entity("product", product_id),
            );
        }
    }

    diagnostics
}

// ============================================================================
// CLI Command
// ============================================================================

#[derive(Parser, PartialEq, Clone, Debug)]
pub struct LintCommand {
    /// Output format: pretty or json
    #[arg(long = "format", short = 'f', default_value = "pretty")]
    pub format: OutputFormat,

    /// Only show errors (hide warnings and info)
    #[arg(long = "errors-only", short = 'e')]
    pub errors_only: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Pretty,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(OutputFormat::Pretty),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!(
                "Invalid format: {}. Valid options are: pretty, json",
                s
            )),
        }
    }
}

impl LintCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        // Get catalog path from first catalog (or default to "billing/v1")
        let catalog_path = ctx
            .manifest
            .catalogs
            .values()
            .next()
            .map(|c| c.catalog_path.as_str())
            .unwrap_or("billing/v1");

        println!(
            "{} Linting catalog at {}...\n",
            style("→").cyan(),
            catalog_path
        );

        let result = lint_all(&ctx.manifest_path, catalog_path);

        match self.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Failed to serialize result: {}", e))?;
                println!("{}", json);
            }
            OutputFormat::Pretty => {
                self.print_pretty(&result);
            }
        }

        if !result.is_valid {
            Err(format!("Lint failed with {} error(s)", result.error_count))
        } else {
            Ok(())
        }
    }

    fn print_pretty(&self, result: &LintResult) {
        for diagnostic in &result.diagnostics {
            // Skip non-errors if --errors-only
            if self.errors_only && diagnostic.severity != DiagnosticSeverity::Error {
                continue;
            }

            let icon = match diagnostic.severity {
                DiagnosticSeverity::Error => style("✗").red(),
                DiagnosticSeverity::Warning => style("⚠").yellow(),
                DiagnosticSeverity::Info => style("ℹ").blue(),
            };

            let severity = match diagnostic.severity {
                DiagnosticSeverity::Error => style("error").red().bold(),
                DiagnosticSeverity::Warning => style("warning").yellow().bold(),
                DiagnosticSeverity::Info => style("info").blue().bold(),
            };

            // Location string
            let location = if let Some(ref file) = diagnostic.file {
                if let Some(line) = diagnostic.line {
                    format!("{}:{}", file, line)
                } else {
                    file.clone()
                }
            } else {
                String::new()
            };

            println!(
                "{} {} [{}]: {}",
                icon,
                severity,
                style(&diagnostic.rule).dim(),
                diagnostic.message
            );

            if !location.is_empty() {
                println!("   {} {}", style("at").dim(), style(location).dim());
            }

            println!();
        }

        // Summary
        println!("{}", style("─".repeat(60)).dim());

        if result.is_valid {
            println!(
                "{} {} {}",
                style("✓").green().bold(),
                style("Lint passed!").green().bold(),
                style(format!(
                    "({} warning(s), {} info)",
                    result.warning_count, result.info_count
                ))
                .dim()
            );
        } else {
            println!(
                "{} {} {}",
                style("✗").red().bold(),
                style("Lint failed!").red().bold(),
                style(format!(
                    "({} error(s), {} warning(s))",
                    result.error_count, result.warning_count
                ))
                .dim()
            );
        }
    }
}
