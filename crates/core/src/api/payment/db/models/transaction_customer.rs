use diesel::prelude::*;
use moneymq_types::x402::transactions::TransactionCustomer;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::payment::db::{PooledConnection, schema::*};

#[derive(Debug, Queryable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = transaction_customers)]
pub struct TransactionCustomerModel {
    pub id: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub label: Option<String>,
    pub address: String,
}

impl From<TransactionCustomerModel> for TransactionCustomer {
    fn from(val: TransactionCustomerModel) -> Self {
        TransactionCustomer {
            label: val.label,
            address: val.address,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = transaction_customers)]
pub struct NewTransactionCustomer<'a> {
    pub created_at: i64,
    pub updated_at: i64,
    pub label: Option<&'a str>,
    pub address: &'a str,
}
impl<'a> NewTransactionCustomer<'a> {
    pub fn new(label: Option<&'a str>, address: &'a str) -> NewTransactionCustomer<'a> {
        let timestamp = chrono::Utc::now().timestamp();
        Self {
            created_at: timestamp,
            updated_at: timestamp,
            label,
            address,
        }
    }
    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<i32> {
        debug!(
            "Inserting transaction customer with address: {}, label: {:?}",
            self.address, self.label
        );
        diesel::insert_into(transaction_customers::table)
            .values(self)
            .on_conflict(transaction_customers::address)
            .do_update()
            .set(transaction_customers::updated_at.eq(self.updated_at))
            .returning(transaction_customers::id)
            .get_result(conn)
    }
}
