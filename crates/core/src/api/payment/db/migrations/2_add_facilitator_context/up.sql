-- Add facilitator_id and is_sandbox columns to facilitated_transactions
-- These columns enable filtering transactions by facilitator and environment

ALTER TABLE facilitated_transactions ADD COLUMN facilitator_id TEXT NOT NULL DEFAULT 'local';
ALTER TABLE facilitated_transactions ADD COLUMN is_sandbox BOOLEAN NOT NULL DEFAULT 1;

-- Create index for efficient filtering by facilitator and environment
CREATE INDEX idx_facilitated_transactions_facilitator
ON facilitated_transactions(facilitator_id, is_sandbox);
