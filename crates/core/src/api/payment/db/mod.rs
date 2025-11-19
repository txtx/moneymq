use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use moneymq_types::x402::transactions::FacilitatedTransaction;
use sha2::{Digest, Sha256};
use solana_keypair::{Keypair, Signer};
use std::str::FromStr;
use tracing::debug;

use crate::api::payment::endpoints::FacilitatorExtraContext;

mod models;
pub mod schema;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/api/payment/db/migrations");

#[cfg(feature = "sqlite")]
type DbConnection = diesel::sqlite::SqliteConnection;
#[cfg(feature = "postgres")]
type DbConnection = diesel::pg::PgConnection;

pub type PooledConnection = diesel::r2d2::PooledConnection<ConnectionManager<DbConnection>>;

pub type DbPool = Pool<ConnectionManager<DbConnection>>;

#[derive(Debug)]
pub struct DbManager {
    control_db_conn: DbPool,
    payment_db_conn: DbPool,
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

/// Calculate SHA256 hash of payment requirement for idempotency
/// Returns a hex-encoded hash string
fn calculate_payment_hash(payment_requirement_base64: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payment_requirement_base64.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Map SPL token mint addresses to currency symbols
fn map_spl_token_to_symbol(mint_address: &str) -> String {
    match mint_address {
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => "USDC".to_string(),
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => "USDT".to_string(),
        "So11111111111111111111111111111111111111112" => "SOL".to_string(),
        _ => mint_address.to_string(),
    }
}

/// Resolve sandbox account addresses to friendly labels (alice, bob, etc.)
fn resolve_sandbox_account_label(address: &str) -> Option<String> {
    // Try to parse the address
    let target_pubkey = solana_pubkey::Pubkey::from_str(address).ok()?;

    const LABELS: &[&str] = &[
        "alice", "bob", "charlie", "david", "eve", "frank", "grace", "heidi", "ivan", "judy",
        "kevin", "laura", "michael", "nancy", "oscar", "peggy", "quinn", "rachel", "steve",
        "trent", "ursula", "victor", "wendy", "xavier", "yvonne", "zach",
    ];

    // Check each known sandbox account
    for (index, label) in LABELS.iter().enumerate() {
        let mut hasher = Sha256::new();
        hasher.update(b"moneymq-user-account");
        hasher.update(index.to_le_bytes());
        let result = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&result);

        let keypair = Keypair::new_from_array(seed);
        if keypair.pubkey() == target_pubkey {
            return Some(label.to_string());
        }
    }

    None
}

impl DbManager {
    pub fn local(database_url: &str) -> DbResult<Self> {
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
        Ok(Self {
            control_db_conn: pool.clone(),
            payment_db_conn: pool,
        })
    }

    pub fn insert_transaction(
        &self,
        verify_request: &moneymq_types::x402::VerifyRequest,
        verify_response: &moneymq_types::x402::VerifyResponse,
        payment_requirement_base64: String,
        verify_request_base64: String,
        verify_response_base64: String,
    ) -> DbResult<()> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        // Extract currency from asset field and map to symbol (USDC, USDT, etc)
        let asset_address = verify_request.payment_requirements.asset.to_string();
        let currency = Some(map_spl_token_to_symbol(&asset_address));

        // Extract customer address from the transaction payload (account_keys[1])
        let customer_address = match &verify_request.payment_payload.payload {
            moneymq_types::x402::ExactPaymentPayload::Solana(payload) => {
                // Decode and extract the customer (payer) from the transaction
                use crate::api::payment::networks::solana::extract_customer_from_transaction;
                extract_customer_from_transaction(&payload.transaction)
                    .ok()
                    .map(|pubkey| pubkey.to_string())
            }
        };

        // Extract product from extra context or derive from resource URL
        let extra_ctx = verify_request
            .payment_requirements
            .extra
            .as_ref()
            .and_then(|extra| {
                serde_json::from_value::<FacilitatorExtraContext>(extra.clone()).ok()
            });

        let product = extra_ctx
            .as_ref()
            .and_then(|ctx| ctx.product.clone())
            .or_else(|| {
                // Fallback: extract endpoint from resource URL
                let path = verify_request.payment_requirements.resource.path();
                if !path.is_empty() && path != "/" {
                    Some(path.trim_start_matches('/').to_string())
                } else {
                    None
                }
            });

        // Find or create customer
        let customer_id = if let Some(address) = customer_address {
            // Try to resolve known sandbox account labels (alice, bob, etc.)
            let customer_label = extra_ctx
                .as_ref()
                .and_then(|ctx| ctx.customer_label.clone())
                .or_else(|| resolve_sandbox_account_label(&address));

            let new_customer = models::transaction_customer::NewTransactionCustomer::new(
                customer_label.as_deref(),
                &address,
            );
            let customer_id = new_customer
                .insert(&mut conn)
                .map_err(DbError::InsertCustomerError)?;
            Some(customer_id)
        } else {
            None
        };

        // Calculate payment hash for idempotency
        let payment_hash = Some(calculate_payment_hash(&payment_requirement_base64));

        // Extract amount from payment requirements
        let amount = verify_request
            .payment_requirements
            .max_amount_required
            .0
            .clone();

        let new_transaction = models::facilitated_transaction::NewFacilitatedTransaction::new(
            customer_id,
            product,
            amount,
            currency,
            payment_requirement_base64,
            Some(verify_request_base64),
            Some(verify_response_base64),
            payment_hash,
        );

        // Handle idempotent inserts - if payment_hash already exists, treat as success
        match new_transaction.insert(&mut conn) {
            Ok(_) => Ok(()),
            Err(diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            )) => {
                debug!("Transaction with payment_hash already exists (idempotent insert)");
                Ok(())
            }
            Err(e) => Err(DbError::InsertTxError(e)),
        }
    }

    /// Check if a transaction with this payment_requirement is already settled
    pub fn is_transaction_already_settled(&self, x402_payment_requirement: &str) -> DbResult<bool> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let payment_hash = calculate_payment_hash(x402_payment_requirement);

        models::facilitated_transaction::is_transaction_already_settled(&mut conn, &payment_hash)
            .map_err(DbError::FindTxError)
    }

    /// Find transaction ID by payment_hash for settlement updates
    /// This is the preferred method for finding transactions to settle
    pub fn find_transaction_id_by_payment_hash(
        &self,
        x402_payment_requirement: &str,
    ) -> DbResult<Option<i32>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let payment_hash = calculate_payment_hash(x402_payment_requirement);

        models::facilitated_transaction::find_transaction_id_by_payment_hash(
            &mut conn,
            &payment_hash,
        )
        .map_err(DbError::FindTxError)
    }

    /// Legacy method - kept for backward compatibility but prefer find_transaction_id_by_payment_hash
    pub fn find_transaction_id_for_settlement_update(
        &self,
        amount: &str,
        x402_payment_requirement: &str,
        extra_ctx: Option<FacilitatorExtraContext>,
    ) -> DbResult<Option<i32>> {
        let mut conn = self
            .payment_db_conn
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
            .payment_db_conn
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
            .payment_db_conn
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
