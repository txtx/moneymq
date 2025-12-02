use diesel::prelude::*;
use moneymq_types::x402::transactions::FacilitatedTransaction;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::payment::db::{PooledConnection, models::TransactionCustomerModel, schema::*};

/// Check if a transaction with payment_hash exists and is already settled
/// Returns true if transaction exists and has both settle_request and settle_response
pub fn is_transaction_already_settled(
    conn: &mut PooledConnection,
    payment_hash: &str,
) -> QueryResult<bool> {
    facilitated_transactions::table
        .filter(facilitated_transactions::payment_hash.eq(payment_hash))
        .filter(facilitated_transactions::x402_settle_request.is_not_null())
        .filter(facilitated_transactions::x402_settle_response.is_not_null())
        .select(facilitated_transactions::id)
        .first::<i32>(conn)
        .optional()
        .map(|result| result.is_some())
}

/// Find transaction by payment_hash for settlement updates
/// Returns the transaction ID if found and not yet settled
pub fn find_transaction_id_by_payment_hash(
    conn: &mut PooledConnection,
    payment_hash: &str,
) -> QueryResult<Option<i32>> {
    facilitated_transactions::table
        .filter(facilitated_transactions::payment_hash.eq(payment_hash))
        .filter(facilitated_transactions::x402_settle_request.is_null())
        .filter(facilitated_transactions::x402_settle_response.is_null())
        .order(facilitated_transactions::created_at.desc())
        .select(facilitated_transactions::id)
        .first::<i32>(conn)
        .optional()
}

/// Legacy method - kept for backward compatibility but prefer find_transaction_id_by_payment_hash
pub fn find_transaction_id_for_settlement_update(
    conn: &mut PooledConnection,
    product: Option<&str>,
    customer_id: Option<i32>,
    amount: &str,
    currency: Option<&str>,
    x402_payment_requirement: &str,
) -> QueryResult<Option<i32>> {
    let mut query = facilitated_transactions::table
        .filter(facilitated_transactions::amount.eq(amount))
        .filter(facilitated_transactions::x402_payment_requirement.eq(x402_payment_requirement))
        .into_boxed();

    if let Some(product) = product {
        query = query.filter(facilitated_transactions::product.eq(product));
    } else {
        query = query.filter(facilitated_transactions::product.is_null());
    }

    if let Some(customer_id) = customer_id {
        query = query.filter(facilitated_transactions::customer_id.eq(customer_id));
    } else {
        query = query.filter(facilitated_transactions::customer_id.is_null());
    }

    if let Some(currency) = currency {
        query = query.filter(facilitated_transactions::currency.eq(currency));
    } else {
        query = query.filter(facilitated_transactions::currency.is_null());
    }

    query = query.filter(facilitated_transactions::x402_settle_request.is_null());
    query = query.filter(facilitated_transactions::x402_settle_response.is_null());

    query
        .order(facilitated_transactions::created_at.desc())
        .select(facilitated_transactions::id)
        .first::<i32>(conn)
        .optional()
}

#[derive(Debug, Queryable, Identifiable, Selectable, Associations, Serialize, Deserialize)]
#[diesel(belongs_to(TransactionCustomerModel, foreign_key = customer_id))]
#[diesel(table_name = facilitated_transactions)]
pub struct FacilitatedTransactionModel {
    pub id: i32,
    pub created_at: i64,
    pub updated_at: i64,
    /// The product name
    pub product: Option<String>,
    /// The customer ID
    pub customer_id: Option<i32>,
    /// The amount as a string
    pub amount: String,
    /// The currency code (e.g., "USDC")
    pub currency: Option<String>,
    /// The transaction status
    pub status: Option<String>,
    /// The Solana transaction signature
    pub signature: Option<String>,
    /// Base64-encoded 402 payment requirement
    pub x402_payment_requirement: String,
    /// Base64-encoded verify request to facilitator
    pub x402_verify_request: Option<String>,
    /// Base64-encoded verify response from facilitator
    pub x402_verify_response: Option<String>,
    /// Base64-encoded settle request to facilitator
    pub x402_settle_request: Option<String>,
    /// Base64-encoded settle response from facilitator
    pub x402_settle_response: Option<String>,
    /// SHA256 hash of x402_payment_requirement for idempotency
    pub payment_hash: Option<String>,
    /// The facilitator ID (subdomain) that processed this transaction
    pub facilitator_id: String,
    /// Whether this transaction was processed in sandbox mode
    pub is_sandbox: bool,
}

#[derive(Debug, Queryable)]
pub struct FacilitatedTransactionWithCustomer {
    pub facilitated: FacilitatedTransactionModel,
    pub customer: Option<TransactionCustomerModel>,
}

impl FacilitatedTransactionWithCustomer {
    pub fn list(
        conn: &mut PooledConnection,
        limit: usize,
        starting_after: Option<i32>,
        facilitator_id: &str,
        is_sandbox: bool,
    ) -> QueryResult<(Vec<FacilitatedTransactionWithCustomer>, bool)> {
        let raw_limit = (limit + 1) as i64;

        let mut rows: Vec<(
            FacilitatedTransactionModel,
            Option<TransactionCustomerModel>,
        )> = facilitated_transactions::table
            .left_join(transaction_customers::table)
            .filter(facilitated_transactions::id.gt(starting_after.unwrap_or(0)))
            .filter(facilitated_transactions::facilitator_id.eq(facilitator_id))
            .filter(facilitated_transactions::is_sandbox.eq(is_sandbox))
            .order(facilitated_transactions::id.asc())
            .limit(raw_limit)
            .load(conn)?;

        let has_more = rows.len() > limit;
        if has_more {
            rows.pop();
        }

        let items = rows
            .into_iter()
            .map(
                |(facilitated, customer)| FacilitatedTransactionWithCustomer {
                    facilitated,
                    customer,
                },
            )
            .collect();

        Ok((items, has_more))
    }
}

impl Into<FacilitatedTransaction> for FacilitatedTransactionWithCustomer {
    fn into(self) -> FacilitatedTransaction {
        FacilitatedTransaction {
            id: self.facilitated.id,
            created_at: self.facilitated.created_at,
            updated_at: self.facilitated.updated_at,
            product: self.facilitated.product,
            customer: self.customer.map(|c| c.into()),
            amount: self.facilitated.amount,
            currency: self.facilitated.currency,
            status: self.facilitated.status,
            signature: self.facilitated.signature,
            x402_payment_requirement: Some(self.facilitated.x402_payment_requirement),
            x402_verify_request: self.facilitated.x402_verify_request,
            x402_verify_response: self.facilitated.x402_verify_response,
            x402_settle_request: self.facilitated.x402_settle_request,
            x402_settle_response: self.facilitated.x402_settle_response,
            facilitator_id: self.facilitated.facilitator_id,
            is_sandbox: self.facilitated.is_sandbox,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = facilitated_transactions)]
pub struct NewFacilitatedTransaction {
    pub product: Option<String>,
    pub customer_id: Option<i32>,
    pub created_at: i64,
    pub updated_at: i64,
    pub amount: String,
    pub currency: Option<String>,
    pub x402_payment_requirement: String,
    pub x402_verify_request: Option<String>,
    pub x402_verify_response: Option<String>,
    pub payment_hash: Option<String>,
    pub facilitator_id: String,
    pub is_sandbox: bool,
}

impl NewFacilitatedTransaction {
    pub fn new(
        customer_id: Option<i32>,
        product: Option<String>,
        amount: String,
        currency: Option<String>,
        x402_payment_requirement: String,
        x402_verify_request: Option<String>,
        x402_verify_response: Option<String>,
        payment_hash: Option<String>,
        facilitator_id: String,
        is_sandbox: bool,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        Self {
            product,
            customer_id,
            created_at: timestamp,
            updated_at: timestamp,
            amount,
            currency,
            x402_payment_requirement,
            x402_verify_request,
            x402_verify_response,
            payment_hash,
            facilitator_id,
            is_sandbox,
        }
    }
    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<usize> {
        debug!(
            "Inserting facilitated transaction with amount: {}, currency: {:?}, product: {:?}, customer_id: {:?}",
            self.amount, self.currency, self.product, self.customer_id
        );
        diesel::insert_into(facilitated_transactions::table)
            .values(self)
            .execute(conn)
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = facilitated_transactions)]
pub struct UpdateFacilitatedTransaction {
    pub status: Option<String>,
    pub signature: Option<String>,
    pub updated_at: i64,
    pub x402_settle_request: Option<String>,
    pub x402_settle_response: Option<String>,
}

impl UpdateFacilitatedTransaction {
    pub fn new(
        status: Option<String>,
        signature: Option<String>,
        x402_settle_request: Option<String>,
        x402_settle_response: Option<String>,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        Self {
            status,
            signature,
            updated_at: timestamp,
            x402_settle_request,
            x402_settle_response,
        }
    }
    pub fn update(&self, conn: &mut PooledConnection, transaction_id: i32) -> QueryResult<usize> {
        debug!(
            "Updating facilitated transaction with id: {}, status: {:?}, signature: {:?}",
            transaction_id, self.status, self.signature
        );
        diesel::update(
            facilitated_transactions::table.filter(facilitated_transactions::id.eq(transaction_id)),
        )
        .set(self)
        .execute(conn)
    }
}
