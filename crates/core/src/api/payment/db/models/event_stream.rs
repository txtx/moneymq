use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api::payment::db::{PooledConnection, schema::event_streams};

#[derive(Debug, Queryable, Identifiable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = event_streams)]
pub struct EventStreamModel {
    pub id: i32,
    pub stream_id: String,
    pub payment_stack_id: String,
    pub is_sandbox: bool,
    pub last_event_id: Option<String>,
    pub last_event_time: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Insertable)]
#[diesel(table_name = event_streams)]
pub struct NewEventStream {
    pub stream_id: String,
    pub payment_stack_id: String,
    pub is_sandbox: bool,
    pub last_event_id: Option<String>,
    pub last_event_time: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl NewEventStream {
    pub fn new(stream_id: String, payment_stack_id: String, is_sandbox: bool) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            stream_id,
            payment_stack_id,
            is_sandbox,
            last_event_id: None,
            last_event_time: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn insert(&self, conn: &mut PooledConnection) -> QueryResult<usize> {
        diesel::insert_into(event_streams::table)
            .values(self)
            .execute(conn)
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = event_streams)]
pub struct UpdateEventStreamCursor {
    pub last_event_id: Option<String>,
    pub last_event_time: Option<i64>,
    pub updated_at: i64,
}

impl UpdateEventStreamCursor {
    pub fn new(event_id: String, event_time: i64) -> Self {
        Self {
            last_event_id: Some(event_id),
            last_event_time: Some(event_time),
            updated_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn update(
        &self,
        conn: &mut PooledConnection,
        stream_id: &str,
        payment_stack_id: &str,
        is_sandbox: bool,
    ) -> QueryResult<usize> {
        diesel::update(
            event_streams::table
                .filter(event_streams::stream_id.eq(stream_id))
                .filter(event_streams::payment_stack_id.eq(payment_stack_id))
                .filter(event_streams::is_sandbox.eq(is_sandbox)),
        )
        .set(self)
        .execute(conn)
    }
}

/// Find or create an event stream
/// Returns the stream model (creating if not exists)
pub fn find_or_create_stream(
    conn: &mut PooledConnection,
    stream_id: &str,
    payment_stack_id: &str,
    is_sandbox: bool,
) -> QueryResult<EventStreamModel> {
    use tracing::{error, info};

    info!(
        stream_id = %stream_id,
        payment_stack_id = %payment_stack_id,
        is_sandbox = %is_sandbox,
        "Looking up event stream in DB"
    );

    // Try to find existing stream
    let existing = event_streams::table
        .filter(event_streams::stream_id.eq(stream_id))
        .filter(event_streams::payment_stack_id.eq(payment_stack_id))
        .filter(event_streams::is_sandbox.eq(is_sandbox))
        .first::<EventStreamModel>(conn)
        .optional()?;

    match existing {
        Some(stream) => {
            info!(
                stream_id = %stream_id,
                db_id = %stream.id,
                last_event_id = ?stream.last_event_id,
                "Found EXISTING event stream in DB"
            );
            Ok(stream)
        }
        None => {
            info!(
                stream_id = %stream_id,
                "Stream NOT found in DB, CREATING new stream"
            );

            // Create new stream
            let new_stream = NewEventStream::new(
                stream_id.to_string(),
                payment_stack_id.to_string(),
                is_sandbox,
            );

            match new_stream.insert(conn) {
                Ok(_) => {
                    info!(stream_id = %stream_id, "Successfully INSERTED new stream into DB");
                }
                Err(e) => {
                    error!(stream_id = %stream_id, error = %e, "FAILED to insert stream into DB");
                    return Err(e);
                }
            }

            // Fetch the newly created stream
            let created = event_streams::table
                .filter(event_streams::stream_id.eq(stream_id))
                .filter(event_streams::payment_stack_id.eq(payment_stack_id))
                .filter(event_streams::is_sandbox.eq(is_sandbox))
                .first::<EventStreamModel>(conn)?;

            info!(
                stream_id = %stream_id,
                db_id = %created.id,
                "Fetched newly created stream from DB"
            );

            Ok(created)
        }
    }
}

/// Update the cursor for a stream after consuming an event
pub fn update_stream_cursor(
    conn: &mut PooledConnection,
    stream_id: &str,
    payment_stack_id: &str,
    is_sandbox: bool,
    event_id: &str,
    event_time: i64,
) -> QueryResult<usize> {
    use tracing::info;

    info!(
        stream_id = %stream_id,
        event_id = %event_id,
        payment_stack_id = %payment_stack_id,
        is_sandbox = %is_sandbox,
        "Updating stream cursor in DB"
    );

    let update = UpdateEventStreamCursor::new(event_id.to_string(), event_time);
    let rows = update.update(conn, stream_id, payment_stack_id, is_sandbox)?;

    info!(
        stream_id = %stream_id,
        event_id = %event_id,
        rows_updated = %rows,
        "Cursor update complete"
    );

    Ok(rows)
}

/// Find a stream by its ID
pub fn find_stream(
    conn: &mut PooledConnection,
    stream_id: &str,
    payment_stack_id: &str,
    is_sandbox: bool,
) -> QueryResult<Option<EventStreamModel>> {
    event_streams::table
        .filter(event_streams::stream_id.eq(stream_id))
        .filter(event_streams::payment_stack_id.eq(payment_stack_id))
        .filter(event_streams::is_sandbox.eq(is_sandbox))
        .first(conn)
        .optional()
}
