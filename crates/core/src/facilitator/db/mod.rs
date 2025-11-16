use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use moneymq_types::x402::transactions::FacilitatedTransaction;
use tracing::debug;

use crate::facilitator::endpoints::FacilitatorExtraContext;

mod models;
pub mod schema;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/facilitator/db/migrations");

#[cfg(feature = "sqlite")]
type DbConnection = diesel::sqlite::SqliteConnection;
#[cfg(feature = "postgres")]
type DbConnection = diesel::pg::PgConnection;

pub type PooledConnection = diesel::r2d2::PooledConnection<ConnectionManager<DbConnection>>;

pub type DbPool = Pool<ConnectionManager<DbConnection>>;

#[derive(Debug)]
pub struct DbManager {
    conn: DbPool,
}

pub type DbResult<T> = Result<T, DbError>;
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database connection error: {0}")]
    ConnectionError(String),
    #[error("Database migration error")]
    MigrationError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Failed to insert customer: {0}")]
    InsertCustomerError(diesel::result::Error),
    #[error("Failed to insert facilitated transaction: {0}")]
    InsertTxError(diesel::result::Error),
    #[error("Failed to find transaction: {0}")]
    FindTxError(diesel::result::Error),
    #[error("Failed to update transaction after settlement: {0}")]
    UpdateTxError(diesel::result::Error),
    #[error("Failed to list transactions: {0}")]
    ListTxError(diesel::result::Error),
}

fn run_migrations(conn: &mut PooledConnection) -> Result<(), DbError> {
    conn.run_pending_migrations(MIGRATIONS)?;
    Ok(())
}

impl DbManager {
    pub fn new(database_url: &str) -> DbResult<Self> {
        debug!("Establishing connection to database at {}", database_url);
        let manager = ConnectionManager::<DbConnection>::new(database_url);
        let pool = Pool::builder().build(manager).unwrap();

        let mut pooled_connection = pool
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        debug!("Running database migrations...");
        if let Err(e) = run_migrations(&mut pooled_connection) {
            debug!("Migrations failure: {}", e);
        }
        Ok(Self { conn: pool })
    }

    pub fn insert_transaction(
        &self,
        extra_ctx: Option<FacilitatorExtraContext>,
        amount: String,
        payment_requirement_base64: String,
        verify_request_base64: String,
        verify_response_base64: String,
    ) -> DbResult<()> {
        let mut conn = self
            .conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let (customer_id, product, currency) = if let Some(extra_ctx) = extra_ctx {
            let customer_id = if let Some(customer_address) = extra_ctx.customer_address {
                let new_customer = models::transaction_customer::NewTransactionCustomer::new(
                    extra_ctx.customer_label.as_deref(),
                    &customer_address,
                );
                let customer_id = new_customer
                    .insert(&mut conn)
                    .map_err(DbError::InsertCustomerError)?;
                Some(customer_id)
            } else {
                None
            };
            (customer_id, extra_ctx.product, extra_ctx.currency)
        } else {
            (None, None, None)
        };

        let new_transaction = models::facilitated_transaction::NewFacilitatedTransaction::new(
            customer_id,
            product,
            amount,
            currency,
            payment_requirement_base64,
            Some(verify_request_base64),
            Some(verify_response_base64),
        );

        new_transaction
            .insert(&mut conn)
            .map_err(DbError::InsertTxError)?;
        Ok(())
    }

    pub fn find_transaction_id_for_settlement_update(
        &self,
        amount: &str,
        x402_payment_requirement: &str,
        extra_ctx: Option<FacilitatorExtraContext>,
    ) -> DbResult<Option<i32>> {
        let mut conn = self
            .conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let customer_id = if let Some(extra_ctx) = &extra_ctx {
            if let Some(customer_address) = &extra_ctx.customer_address {
                models::transaction_customer::find_customer_by_address(&mut conn, customer_address)
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        };

        models::facilitated_transaction::find_transaction_id_for_settlement_update(
            &mut conn,
            extra_ctx
                .as_ref()
                .and_then(|ctx| ctx.product.clone())
                .as_deref(),
            customer_id,
            amount,
            extra_ctx.as_ref().and_then(|ctx| ctx.currency.as_deref()),
            x402_payment_requirement,
        )
        .map_err(DbError::FindTxError)
    }

    pub fn update_transaction_after_settlement(
        &self,
        transaction_id: i32,
        status: Option<String>,
        signature: Option<String>,
        settle_request_base64: Option<String>,
        settle_response_base64: Option<String>,
    ) -> DbResult<()> {
        let mut conn = self
            .conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let update = models::facilitated_transaction::UpdateFacilitatedTransaction::new(
            status,
            signature,
            settle_request_base64,
            settle_response_base64,
        );

        update
            .update(&mut conn, transaction_id)
            .map_err(DbError::UpdateTxError)?;
        Ok(())
    }

    pub fn list_transactions(
        &self,
        limit: usize,
        starting_after: Option<i32>,
    ) -> DbResult<(Vec<FacilitatedTransaction>, bool)> {
        let mut conn = self
            .conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::facilitated_transaction::FacilitatedTransactionWithCustomer::list(
            &mut conn,
            limit,
            starting_after,
        )
        .map_err(DbError::ListTxError)
        .map(|(txs_with_customer, has_more)| {
            (
                txs_with_customer
                    .into_iter()
                    .map(|tx_with_customer| tx_with_customer.into())
                    .collect(),
                has_more,
            )
        })
    }
}
