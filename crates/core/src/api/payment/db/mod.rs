use std::str::FromStr;

use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use moneymq_types::x402::transactions::FacilitatedTransaction;
use sha2::{Digest, Sha256};
use solana_keypair::{Keypair, Signer};
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

/// Custom connection initializer for SQLite
/// Enables WAL mode and sets busy_timeout for better concurrency
#[cfg(feature = "sqlite")]
#[derive(Debug)]
struct SqliteConnectionCustomizer;

#[cfg(feature = "sqlite")]
impl diesel::r2d2::CustomizeConnection<DbConnection, diesel::r2d2::Error>
    for SqliteConnectionCustomizer
{
    fn on_acquire(&self, conn: &mut DbConnection) -> Result<(), diesel::r2d2::Error> {
        use diesel::connection::SimpleConnection;
        // WAL mode allows concurrent reads while writing
        // busy_timeout waits up to 5 seconds for locks instead of failing immediately
        conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
            .map_err(diesel::r2d2::Error::QueryError)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct DbManager {
    #[allow(dead_code)]
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
    #[error("Failed to insert cloud event: {0}")]
    InsertEventError(diesel::result::Error),
    #[error("Failed to query events: {0}")]
    QueryEventError(diesel::result::Error),
    #[error("Failed to manage event stream: {0}")]
    EventStreamError(diesel::result::Error),
}

fn run_migrations(conn: &mut PooledConnection) -> Result<(), DbError> {
    conn.run_pending_migrations(MIGRATIONS)?;
    Ok(())
}

/// Calculate SHA256 hash of transaction message (without signatures) for idempotency
/// Returns a hex-encoded hash string
/// For Solana, this hashes just the transaction message (instructions, accounts, blockhash)
/// excluding signatures, so verify and settle operations can be matched
fn calculate_payment_hash(transaction: &str) -> Result<String, String> {
    use crate::api::payment::networks::solana::extract_transaction_message_bytes;

    let message_bytes = extract_transaction_message_bytes(transaction)
        .map_err(|e| format!("Failed to extract transaction message: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(&message_bytes);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Map SPL token mint addresses to currency symbols
fn map_spl_token_to_symbol(mint_address: &str) -> String {
    match mint_address {
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => "USDC".to_string(),
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

        #[cfg(feature = "sqlite")]
        let pool = Pool::builder()
            .connection_customizer(Box::new(SqliteConnectionCustomizer))
            .build(manager)
            .unwrap();

        #[cfg(feature = "postgres")]
        let pool = Pool::builder().build(manager).unwrap();

        let mut pooled_connection = pool
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        debug!("Running database migrations...");
        match run_migrations(&mut pooled_connection) {
            Ok(_) => debug!("Database migrations completed successfully"),
            Err(e) => {
                tracing::error!(
                    "Database migrations failed: {}. Consider deleting payments.sqlite and restarting.",
                    e
                );
            }
        }
        Ok(Self {
            control_db_conn: pool.clone(),
            payment_db_conn: pool,
        })
    }

    pub fn insert_transaction(
        &self,
        verify_request: &moneymq_types::x402::VerifyRequest,
        _verify_response: &moneymq_types::x402::VerifyResponse,
        payment_requirement_base64: String,
        verify_request_base64: String,
        verify_response_base64: String,
        payment_stack_id: &str,
        is_sandbox: bool,
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

        // Calculate payment hash for idempotency based on the transaction message (without signatures)
        // This ensures verify and settle operations can be matched even if signatures differ
        let payment_hash = match &verify_request.payment_payload.payload {
            moneymq_types::x402::ExactPaymentPayload::Solana(payload) => {
                match calculate_payment_hash(&payload.transaction) {
                    Ok(hash) => Some(hash),
                    Err(e) => {
                        debug!("Failed to calculate payment hash: {}", e);
                        None
                    }
                }
            }
        };

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
            payment_stack_id.to_string(),
            is_sandbox,
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

    /// Check if a transaction with this transaction payload is already settled
    pub fn is_transaction_already_settled(&self, x402_transaction: &str) -> DbResult<bool> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let payment_hash =
            calculate_payment_hash(x402_transaction).map_err(DbError::ConnectionError)?;

        models::facilitated_transaction::is_transaction_already_settled(&mut conn, &payment_hash)
            .map_err(DbError::FindTxError)
    }

    /// Find transaction ID by payment_hash for settlement updates
    /// This is the preferred method for finding transactions to settle
    pub fn find_transaction_id_by_payment_hash(
        &self,
        x402_transaction: &str,
    ) -> DbResult<Option<i32>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let payment_hash =
            calculate_payment_hash(x402_transaction).map_err(DbError::ConnectionError)?;

        models::facilitated_transaction::find_transaction_id_by_payment_hash(
            &mut conn,
            &payment_hash,
        )
        .map_err(DbError::FindTxError)
    }

    /// Find full transaction info by payment_hash (channel_id)
    /// This is used when processors publish events to channels to look up payment context
    pub fn find_transaction_by_payment_hash(
        &self,
        payment_hash: &str,
    ) -> DbResult<Option<moneymq_types::x402::transactions::FacilitatedTransaction>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::facilitated_transaction::find_transaction_by_payment_hash(&mut conn, payment_hash)
            .map_err(DbError::FindTxError)
            .map(|opt| opt.map(|tx_with_customer| tx_with_customer.into()))
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
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> DbResult<(Vec<FacilitatedTransaction>, bool)> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::facilitated_transaction::FacilitatedTransactionWithCustomer::list(
            &mut conn,
            limit,
            starting_after,
            payment_stack_id,
            is_sandbox,
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

    // ==================== Event Stream Methods ====================

    /// Insert a CloudEvent into the database for replay
    pub fn insert_cloud_event(
        &self,
        event_id: String,
        event_type: String,
        event_source: String,
        event_time: i64,
        data_json: String,
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> DbResult<()> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        let new_event = models::cloud_event::NewCloudEvent::new(
            event_id,
            event_type,
            event_source,
            event_time,
            data_json,
            payment_stack_id.to_string(),
            is_sandbox,
        );

        new_event
            .insert(&mut conn)
            .map_err(DbError::InsertEventError)?;
        Ok(())
    }

    /// Get events after a cursor for replay (for stateful streams)
    pub fn get_events_after_cursor(
        &self,
        cursor_event_id: &str,
        payment_stack_id: &str,
        is_sandbox: bool,
        limit: i64,
    ) -> DbResult<Vec<models::CloudEventModel>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::cloud_event::get_events_after_cursor(
            &mut conn,
            cursor_event_id,
            payment_stack_id,
            is_sandbox,
            limit,
        )
        .map_err(DbError::QueryEventError)
    }

    /// Get the last N events for initial replay
    pub fn get_last_events(
        &self,
        payment_stack_id: &str,
        is_sandbox: bool,
        limit: i64,
    ) -> DbResult<Vec<models::CloudEventModel>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::cloud_event::get_last_events(&mut conn, payment_stack_id, is_sandbox, limit)
            .map_err(DbError::QueryEventError)
    }

    /// Find or create a stateful event stream
    pub fn find_or_create_event_stream(
        &self,
        stream_id: &str,
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> DbResult<models::EventStreamModel> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::event_stream::find_or_create_stream(
            &mut conn,
            stream_id,
            payment_stack_id,
            is_sandbox,
        )
        .map_err(DbError::EventStreamError)
    }

    /// Update the cursor for a stateful stream after consuming an event
    /// Returns the number of rows updated (should be 1 if stream exists)
    pub fn update_event_stream_cursor(
        &self,
        stream_id: &str,
        payment_stack_id: &str,
        is_sandbox: bool,
        event_id: &str,
        event_time: i64,
    ) -> DbResult<usize> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::event_stream::update_stream_cursor(
            &mut conn,
            stream_id,
            payment_stack_id,
            is_sandbox,
            event_id,
            event_time,
        )
        .map_err(DbError::EventStreamError)
    }

    /// Find a stateful event stream by ID
    pub fn find_event_stream(
        &self,
        stream_id: &str,
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> DbResult<Option<models::EventStreamModel>> {
        let mut conn = self
            .payment_db_conn
            .get()
            .map_err(|e| DbError::ConnectionError(e.to_string()))?;

        models::event_stream::find_stream(&mut conn, stream_id, payment_stack_id, is_sandbox)
            .map_err(DbError::EventStreamError)
    }
}
