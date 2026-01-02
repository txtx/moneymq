use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api::payment::db::{PooledConnection, schema::cloud_events};

#[derive(Debug, Queryable, Identifiable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = cloud_events)]
pub struct CloudEventModel {
    pub id: i32,
    pub event_id: String,
    pub event_type: String,
    pub event_source: String,
    pub event_time: i64,
    pub data_json: String,
    pub payment_stack_id: String,
    pub is_sandbox: bool,
    pub created_at: i64,
}

#[derive(Insertable)]
#[diesel(table_name = cloud_events)]
pub struct NewCloudEvent {
    pub event_id: String,
    pub event_type: String,
    pub event_source: String,
    pub event_time: i64,
    pub data_json: String,
    pub payment_stack_id: String,
    pub is_sandbox: bool,
    pub created_at: i64,
}

impl NewCloudEvent {
    pub fn new(
        event_id: String,
        event_type: String,
        event_source: String,
        event_time: i64,
        data_json: String,
        payment_stack_id: String,
        is_sandbox: bool,
    ) -> Self {
        let created_at = chrono::Utc::now().timestamp_millis();
        Self {
            event_id,
            event_type,
            event_source,
            event_time,
            data_json,
            payment_stack_id,
            is_sandbox,
            created_at,
        }
    }

    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<usize> {
        diesel::insert_into(cloud_events::table)
            .values(self)
            .execute(conn)
    }
}

/// Get events after a given cursor (event_id) for replay
/// Returns events in chronological order
pub fn get_events_after_cursor(
    conn: &mut PooledConnection,
    cursor_event_id: &str,
    payment_stack_id: &str,
    is_sandbox: bool,
    limit: i64,
) -> QueryResult<Vec<CloudEventModel>> {
    // First, find the created_at of the cursor event
    let cursor_time: Option<i64> = cloud_events::table
        .filter(cloud_events::event_id.eq(cursor_event_id))
        .filter(cloud_events::payment_stack_id.eq(payment_stack_id))
        .filter(cloud_events::is_sandbox.eq(is_sandbox))
        .select(cloud_events::created_at)
        .first(conn)
        .optional()?;

    match cursor_time {
        Some(time) => {
            // Get events after the cursor's created_at
            cloud_events::table
                .filter(cloud_events::payment_stack_id.eq(payment_stack_id))
                .filter(cloud_events::is_sandbox.eq(is_sandbox))
                .filter(cloud_events::created_at.gt(time))
                .order(cloud_events::created_at.asc())
                .limit(limit)
                .load(conn)
        }
        None => {
            // Cursor not found, return empty
            Ok(vec![])
        }
    }
}

/// Get the last N events for initial replay
pub fn get_last_events(
    conn: &mut PooledConnection,
    payment_stack_id: &str,
    is_sandbox: bool,
    limit: i64,
) -> QueryResult<Vec<CloudEventModel>> {
    cloud_events::table
        .filter(cloud_events::payment_stack_id.eq(payment_stack_id))
        .filter(cloud_events::is_sandbox.eq(is_sandbox))
        .order(cloud_events::created_at.desc())
        .limit(limit)
        .load::<CloudEventModel>(conn)
        .map(|mut events| {
            events.reverse(); // Return in chronological order
            events
        })
}

/// Find event by event_id
#[allow(dead_code)]
pub fn find_by_event_id(
    conn: &mut PooledConnection,
    event_id: &str,
    payment_stack_id: &str,
    is_sandbox: bool,
) -> QueryResult<Option<CloudEventModel>> {
    cloud_events::table
        .filter(cloud_events::event_id.eq(event_id))
        .filter(cloud_events::payment_stack_id.eq(payment_stack_id))
        .filter(cloud_events::is_sandbox.eq(is_sandbox))
        .first(conn)
        .optional()
}
