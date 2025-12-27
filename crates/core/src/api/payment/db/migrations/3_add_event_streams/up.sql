------------------------------------------------------------
-- cloud_events: Stores all CloudEvents for replay functionality
------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cloud_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- CloudEvent standard fields
    event_id TEXT NOT NULL UNIQUE,          -- CloudEvent id (UUID)
    event_type TEXT NOT NULL,               -- e.g., "mq.money.payment.settlement.succeeded"
    event_source TEXT NOT NULL,             -- e.g., "moneymq/payment/settle"
    event_time TIMESTAMP NOT NULL,          -- CloudEvent time
    -- Event data
    data_json TEXT NOT NULL,                -- Full CloudEvent envelope as JSON
    -- Context
    payment_stack_id TEXT NOT NULL,
    is_sandbox BOOL NOT NULL DEFAULT FALSE,
    -- Timestamps
    created_at TIMESTAMP NOT NULL
);

-- Index for efficient replay queries
CREATE INDEX idx_cloud_events_stack_time ON cloud_events(payment_stack_id, is_sandbox, created_at);
CREATE INDEX idx_cloud_events_event_id ON cloud_events(event_id);

------------------------------------------------------------
-- event_streams: Tracks stateful stream cursors
------------------------------------------------------------
CREATE TABLE IF NOT EXISTS event_streams (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Stream identity (deterministic, provided by client)
    stream_id TEXT NOT NULL,
    -- Context scoping
    payment_stack_id TEXT NOT NULL,
    is_sandbox BOOL NOT NULL DEFAULT FALSE,
    -- Cursor tracking
    last_event_id TEXT,                     -- ID of the last consumed event
    last_event_time TIMESTAMP,              -- Time of last consumed event
    -- Timestamps
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    -- Ensure unique stream per stack/sandbox combination
    UNIQUE(stream_id, payment_stack_id, is_sandbox)
);

CREATE INDEX idx_event_streams_lookup ON event_streams(stream_id, payment_stack_id, is_sandbox);
