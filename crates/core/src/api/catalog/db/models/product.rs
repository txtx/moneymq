use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::api::catalog::db::{PooledConnection, schema::*};

#[derive(Debug, Queryable, Identifiable, Selectable, Serialize, Deserialize, Clone)]
#[diesel(table_name = products)]
pub struct ProductModel {
    pub id: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub payment_stack_id: String,
    pub product_id: String,
    pub name: String,
    pub description: Option<String>,
    pub product_type: String,
    pub unit_label: Option<String>,
    pub active: bool,
    pub metadata: Option<String>,
    pub is_sandbox: bool,
}

#[derive(Debug, Queryable)]
pub struct ProductWithPrices {
    pub product: ProductModel,
    pub prices: Vec<super::price::PriceModel>,
}

impl ProductModel {
    /// Find a product by payment_stack_id and product_id
    pub fn find_by_product_id(
        conn: &mut PooledConnection,
        payment_stack_id: &str,
        product_id: &str,
        is_sandbox: bool,
    ) -> QueryResult<Option<ProductModel>> {
        products::table
            .filter(products::payment_stack_id.eq(payment_stack_id))
            .filter(products::product_id.eq(product_id))
            .filter(products::is_sandbox.eq(is_sandbox))
            .first::<ProductModel>(conn)
            .optional()
    }

    /// List all products for a payment stack
    pub fn list_by_payment_stack(
        conn: &mut PooledConnection,
        payment_stack_id: &str,
        is_sandbox: bool,
        active_only: bool,
    ) -> QueryResult<Vec<ProductModel>> {
        let mut query = products::table
            .filter(products::payment_stack_id.eq(payment_stack_id))
            .filter(products::is_sandbox.eq(is_sandbox))
            .into_boxed();

        if active_only {
            query = query.filter(products::active.eq(true));
        }

        query
            .order(products::created_at.desc())
            .load::<ProductModel>(conn)
    }

    /// Delete a product by ID
    pub fn delete(conn: &mut PooledConnection, product_db_id: i32) -> QueryResult<usize> {
        diesel::delete(products::table.filter(products::id.eq(product_db_id))).execute(conn)
    }
}

#[derive(Insertable, Debug)]
#[diesel(table_name = products)]
pub struct NewProduct {
    pub created_at: i64,
    pub updated_at: i64,
    pub payment_stack_id: String,
    pub product_id: String,
    pub name: String,
    pub description: Option<String>,
    pub product_type: String,
    pub unit_label: Option<String>,
    pub active: bool,
    pub metadata: Option<String>,
    pub is_sandbox: bool,
}

impl NewProduct {
    pub fn new(
        payment_stack_id: String,
        product_id: String,
        name: String,
        description: Option<String>,
        product_type: String,
        unit_label: Option<String>,
        active: bool,
        metadata: Option<String>,
        is_sandbox: bool,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp_millis();
        Self {
            created_at: timestamp,
            updated_at: timestamp,
            payment_stack_id,
            product_id,
            name,
            description,
            product_type,
            unit_label,
            active,
            metadata,
            is_sandbox,
        }
    }

    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<i32> {
        debug!("Inserting product: {} ({})", self.name, self.product_id);
        diesel::insert_into(products::table)
            .values(self)
            .returning(products::id)
            .get_result(conn)
    }

    /// Insert or update (upsert) a product
    pub fn upsert(&self, conn: &mut PooledConnection) -> QueryResult<i32> {
        // Try to find existing product
        let existing = products::table
            .filter(products::payment_stack_id.eq(&self.payment_stack_id))
            .filter(products::product_id.eq(&self.product_id))
            .filter(products::is_sandbox.eq(self.is_sandbox))
            .select(products::id)
            .first::<i32>(conn)
            .optional()?;

        match existing {
            Some(id) => {
                // Update existing product
                let update = UpdateProduct {
                    updated_at: chrono::Utc::now().timestamp_millis(),
                    name: Some(self.name.clone()),
                    description: self.description.clone(),
                    product_type: Some(self.product_type.clone()),
                    unit_label: self.unit_label.clone(),
                    active: Some(self.active),
                    metadata: self.metadata.clone(),
                };
                update.update(conn, id)?;
                Ok(id)
            }
            None => {
                // Insert new product
                self.insert(conn)
            }
        }
    }
}

#[derive(AsChangeset, Debug)]
#[diesel(table_name = products)]
pub struct UpdateProduct {
    pub updated_at: i64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub product_type: Option<String>,
    pub unit_label: Option<String>,
    pub active: Option<bool>,
    pub metadata: Option<String>,
}

impl UpdateProduct {
    pub fn update(&self, conn: &mut PooledConnection, product_db_id: i32) -> QueryResult<usize> {
        debug!("Updating product with id: {}", product_db_id);
        diesel::update(products::table.filter(products::id.eq(product_db_id)))
            .set(self)
            .execute(conn)
    }
}
