use rmcp::{ServiceExt, transport::stdio};

use crate::moneymq::MoneyMqMcp;

mod moneymq;
pub mod yaml_util;

#[derive(PartialEq, Clone, Debug, Default)]
pub struct McpOptions {}

pub async fn run_server(_opts: &McpOptions) -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let schema = rmcp::schemars::schema_for!(moneymq::CatalogRequest);
    let schema_json = serde_json::to_string_pretty(&schema).unwrap();
    println!("Catalog request schema: {}", schema_json);

    tracing::info!("Starting MCP server");

    let service = MoneyMqMcp::new()
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })
        .map_err(|e| e.to_string())?;

    service.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}
