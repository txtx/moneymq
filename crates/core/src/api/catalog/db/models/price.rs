use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::catalog::db::{PooledConnection, schema::*};

#[derive(
    Debug, Queryable, Identifiable, Selectable, Associations, Serialize, Deserialize, Clone,
)]
#[diesel(belongs_to(super::product::ProductModel, foreign_key = product_id))]
#[diesel(table_name = prices)]
pub struct PriceModel {
    pub id: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub product_id: i32,
    pub price_id: Option<String>,
    pub pricing_type: String,
    pub currency: String,
    pub unit_amount: i64,
    pub recurring_interval: Option<String>,
    pub recurring_interval_count: Option<i32>,
    pub active: bool,
    pub metadata: Option<String>,
}

impl PriceModel {
    /// List all prices for a product
    pub fn list_by_product(
        conn: &mut PooledConnection,
        product_db_id: i32,
        active_only: bool,
    ) -> QueryResult<Vec<PriceModel>> {
        let mut query = prices::table
            .filter(prices::product_id.eq(product_db_id))
            .into_boxed();

        if active_only {
            query = query.filter(prices::active.eq(true));
        }

        query
            .order(prices::created_at.desc())
            .load::<PriceModel>(conn)
    }

    /// Find a price by price_id
    pub fn find_by_price_id(
        conn: &mut PooledConnection,
        price_id: &str,
    ) -> QueryResult<Option<PriceModel>> {
        prices::table
            .filter(prices::price_id.eq(price_id))
            .first::<PriceModel>(conn)
            .optional()
    }

    /// Delete a price by ID
    pub fn delete(conn: &mut PooledConnection, price_db_id: i32) -> QueryResult<usize> {
        diesel::delete(prices::table.filter(prices::id.eq(price_db_id))).execute(conn)
    }

    /// Delete all prices for a product
    pub fn delete_by_product(
        conn: &mut PooledConnection,
        product_db_id: i32,
    ) -> QueryResult<usize> {
        diesel::delete(prices::table.filter(prices::product_id.eq(product_db_id))).execute(conn)
    }
}

#[derive(Insertable, Debug)]
#[diesel(table_name = prices)]
pub struct NewPrice {
    pub created_at: i64,
    pub updated_at: i64,
    pub product_id: i32,
    pub price_id: Option<String>,
    pub pricing_type: String,
    pub currency: String,
    pub unit_amount: i64,
    pub recurring_interval: Option<String>,
    pub recurring_interval_count: Option<i32>,
    pub active: bool,
    pub metadata: Option<String>,
}

impl NewPrice {
    pub fn new(
        product_id: i32,
        price_id: Option<String>,
        pricing_type: String,
        currency: String,
        unit_amount: i64,
        recurring_interval: Option<String>,
        recurring_interval_count: Option<i32>,
        active: bool,
        metadata: Option<String>,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        Self {
            created_at: timestamp,
            updated_at: timestamp,
            product_id,
            price_id,
            pricing_type,
            currency,
            unit_amount,
            recurring_interval,
            recurring_interval_count,
            active,
            metadata,
        }
    }

    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<i32> {
        debug!(
            "Inserting price: {} {} ({})",
            self.unit_amount, self.currency, self.pricing_type
        );
        diesel::insert_into(prices::table)
            .values(self)
            .returning(prices::id)
            .get_result(conn)
    }
}

#[derive(AsChangeset, Debug)]
#[diesel(table_name = prices)]
pub struct UpdatePrice {
    pub updated_at: i64,
    pub pricing_type: Option<String>,
    pub currency: Option<String>,
    pub unit_amount: Option<i64>,
    pub recurring_interval: Option<String>,
    pub recurring_interval_count: Option<i32>,
    pub active: Option<bool>,
    pub metadata: Option<String>,
}

impl UpdatePrice {
    pub fn update(&self, conn: &mut PooledConnection, price_db_id: i32) -> QueryResult<usize> {
        debug!("Updating price with id: {}", price_db_id);
        diesel::update(prices::table.filter(prices::id.eq(price_db_id)))
            .set(self)
            .execute(conn)
    }
}
