pub mod account;
pub mod common;
pub mod meters;
pub mod prices;
pub mod products;

// Re-export commonly used functions
pub use account::{AccountInfo, get_account_info};
pub use meters::download_meters;
pub use prices::create_price;
pub use products::{create_product, download_catalog, update_product};

#[cfg(test)]
mod tests {
    use moneymq_types::Catalog;

    #[tokio::test]
    async fn test_catalog_structure() {
        // This test verifies the Catalog structure
        let catalog = Catalog::new(vec![], "stripe".to_string());
        assert_eq!(catalog.total_count, 0);
        assert_eq!(catalog.products.len(), 0);
        assert_eq!(catalog.provider, "stripe");
    }
}
