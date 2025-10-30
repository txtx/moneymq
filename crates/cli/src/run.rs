use std::fs;

use moneymq_types::Product;

use crate::Context;

#[derive(Debug, Clone, PartialEq, clap::Args)]
pub struct RunCommand {
    /// Port to run the server on
    #[arg(short, long, default_value = "8488")]
    pub port: u16,

    /// Use sandbox mode (serve sandbox external IDs)
    #[arg(long)]
    pub sandbox: bool,
}

impl RunCommand {
    pub async fn execute(&self, ctx: &Context) -> Result<(), String> {
        println!("🚀 Starting MoneyMQ Provider Server\n");

        // Load products from catalog directory
        let catalog_dir = ctx.manifest_path.join("catalog");
        if !catalog_dir.exists() {
            return Err(format!(
                "Catalog directory not found: {}\nRun 'moneymq init' or 'moneymq catalog sync' first",
                catalog_dir.display()
            ));
        }

        println!("📂 Loading products from {}", catalog_dir.display());

        let mut products = Vec::new();
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
                        products.push(product);
                    }
                    Err(e) => {
                        eprintln!("⚠️  Warning: Failed to parse {}: {}", path.display(), e);
                        eprintln!("    Skipping this file.");
                    }
                }
            }
        }

        if products.is_empty() {
            return Err("No products found in catalog directory".to_string());
        }

        println!("✓ Loaded {} products\n", products.len());

        let mode = if self.sandbox {
            "sandbox"
        } else {
            "production"
        };
        println!("🔧 Mode: {}", mode);
        println!("🌐 Port: {}\n", self.port);

        println!("📡 API Endpoints:");
        println!("  GET http://localhost:{}/v1/products", self.port);
        println!("  GET http://localhost:{}/v1/prices", self.port);
        println!("  GET http://localhost:{}/health\n", self.port);

        println!("💡 Tip: Use this as your Stripe API endpoint for local development");
        println!("   Set STRIPE_API_BASE=http://localhost:{}", self.port);
        println!();
        println!("Press Ctrl+C to stop the server\n");

        // Initialize tracing
        tracing_subscriber::fmt::init();

        // Start the server
        moneymq_core::provider::start_provider(products, self.port, self.sandbox)
            .await
            .map_err(|e| format!("Failed to start server: {}", e))?;

        Ok(())
    }
}
