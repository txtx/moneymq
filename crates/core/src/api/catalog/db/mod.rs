use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use tracing::debug;

pub mod models;
pub mod schema;

pub use models::{NewPrice, NewProduct, PriceModel, ProductModel, UpdatePrice, UpdateProduct};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/api/catalog/db/migrations");

#[cfg(feature = "sqlite")]
type DbConnection = diesel::sqlite::SqliteConnection;
#[cfg(feature = "postgres")]
type DbConnection = diesel::pg::PgConnection;

pub type PooledConnection = diesel::r2d2::PooledConnection<ConnectionManager<DbConnection>>;

pub type DbPool = Pool<ConnectionManager<DbConnection>>;

#[derive(Debug)]
pub struct CatalogDbManager {
    db_conn: DbPool,
}

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database connection error: {0}")]
    ConnectionError(String),
    #[error("Database migration error")]
    MigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Failed to insert product: {0}")]
    InsertProductError(diesel::result::Error),
    #[error("Failed to insert price: {0}")]
    InsertPriceError(diesel::result::Error),
    #[error("Failed to find product: {0}")]
    FindProductError(diesel::result::Error),
    #[error("Failed to update product: {0}")]
    UpdateProductError(diesel::result::Error),
    #[error("Failed to delete product: {0}")]
    DeleteProductError(diesel::result::Error),
    #[error("Failed to list products: {0}")]
    ListProductsError(diesel::result::Error),
    #[error("Failed to list prices: {0}")]
    ListPricesError(diesel::result::Error),
}

fn run_migrations(conn: &mut PooledConnection) -> Result<(), DbError> {
    conn.run_pending_migrations(MIGRATIONS)?;
    Ok(())
}

impl CatalogDbManager {
    pub fn new(database_url: &str) -> DbResult<Self> {
        debug!(
            "Establishing catalog database connection at {}",
            database_url
        );
        let manager = ConnectionManager::<DbConnection>::new(database_url);
        let pool = Pool::builder().build(manager).unwrap();

        let mut pooled_connection = pool
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        debug!("Running catalog database migrations...");
        if let Err(e) = run_migrations(&mut pooled_connection) {
            debug!("Catalog migrations failure: {}", e);
        }

        Ok(Self { db_conn: pool })
    }

    /// Upsert a product (insert or update)
    pub fn upsert_product(
        &self,
        payment_stack_id: &str,
        product_id: &str,
        name: &str,
        description: Option<&str>,
        product_type: &str,
        unit_label: Option<&str>,
        active: bool,
        metadata: Option<&str>,
        is_sandbox: bool,
    ) -> DbResult<i32> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let new_product = NewProduct::new(
            payment_stack_id.to_string(),
            product_id.to_string(),
            name.to_string(),
            description.map(|s| s.to_string()),
            product_type.to_string(),
            unit_label.map(|s| s.to_string()),
            active,
            metadata.map(|s| s.to_string()),
            is_sandbox,
        );

        new_product
            .upsert(&mut conn)
            .map_err(DbError::InsertProductError)
    }

    /// Insert a price for a product
    pub fn insert_price(
        &self,
        product_db_id: i32,
        price_id: Option<&str>,
        pricing_type: &str,
        currency: &str,
        unit_amount: i64,
        recurring_interval: Option<&str>,
        recurring_interval_count: Option<i32>,
        active: bool,
        metadata: Option<&str>,
    ) -> DbResult<i32> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let new_price = NewPrice::new(
            product_db_id,
            price_id.map(|s| s.to_string()),
            pricing_type.to_string(),
            currency.to_string(),
            unit_amount,
            recurring_interval.map(|s| s.to_string()),
            recurring_interval_count,
            active,
            metadata.map(|s| s.to_string()),
        );

        new_price
            .insert(&mut conn)
            .map_err(DbError::InsertPriceError)
    }

    /// List all products for a payment stack
    pub fn list_products(
        &self,
        payment_stack_id: &str,
        is_sandbox: bool,
        active_only: bool,
    ) -> DbResult<Vec<ProductModel>> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        ProductModel::list_by_payment_stack(&mut conn, payment_stack_id, is_sandbox, active_only)
            .map_err(DbError::ListProductsError)
    }

    /// List all prices for a product
    pub fn list_prices(&self, product_db_id: i32, active_only: bool) -> DbResult<Vec<PriceModel>> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        PriceModel::list_by_product(&mut conn, product_db_id, active_only)
            .map_err(DbError::ListPricesError)
    }

    /// Find a product by product_id
    pub fn find_product(
        &self,
        payment_stack_id: &str,
        product_id: &str,
        is_sandbox: bool,
    ) -> DbResult<Option<ProductModel>> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        ProductModel::find_by_product_id(&mut conn, payment_stack_id, product_id, is_sandbox)
            .map_err(DbError::FindProductError)
    }

    /// Delete a product and all its prices
    pub fn delete_product(&self, product_db_id: i32) -> DbResult<()> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        // Prices are deleted automatically via ON DELETE CASCADE
        ProductModel::delete(&mut conn, product_db_id).map_err(DbError::DeleteProductError)?;
        Ok(())
    }

    /// Delete all prices for a product (useful before reinserting)
    pub fn delete_prices_for_product(&self, product_db_id: i32) -> DbResult<()> {
        let mut conn = self
            .db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        PriceModel::delete_by_product(&mut conn, product_db_id)
            .map_err(DbError::DeleteProductError)?;
        Ok(())
    }

    /// Sync a product with its prices (upsert product, delete old prices, insert new prices)
    pub fn sync_product_with_prices(
        &self,
        payment_stack_id: &str,
        product_id: &str,
        name: &str,
        description: Option<&str>,
        product_type: &str,
        unit_label: Option<&str>,
        active: bool,
        metadata: Option<&str>,
        is_sandbox: bool,
        prices: Vec<(
            Option<&str>, // price_id
            &str,         // pricing_type
            &str,         // currency
            i64,          // unit_amount
            Option<&str>, // recurring_interval
            Option<i32>,  // recurring_interval_count
            bool,         // active
            Option<&str>, // metadata
        )>,
    ) -> DbResult<i32> {
        // Upsert the product
        let product_db_id = self.upsert_product(
            payment_stack_id,
            product_id,
            name,
            description,
            product_type,
            unit_label,
            active,
            metadata,
            is_sandbox,
        )?;

        // Delete existing prices
        self.delete_prices_for_product(product_db_id)?;

        // Insert new prices
        for (
            price_id,
            pricing_type,
            currency,
            unit_amount,
            recurring_interval,
            recurring_interval_count,
            price_active,
            price_metadata,
        ) in prices
        {
            self.insert_price(
                product_db_id,
                price_id,
                pricing_type,
                currency,
                unit_amount,
                recurring_interval,
                recurring_interval_count,
                price_active,
                price_metadata,
            )?;
        }

        Ok(product_db_id)
    }
}
