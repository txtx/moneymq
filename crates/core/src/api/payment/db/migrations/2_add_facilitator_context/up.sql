-- Add payment_stack_id and is_sandbox columns to facilitated_transactions
-- These columns enable filtering transactions by payment stack and environment

ALTER TABLE facilitated_transactions ADD COLUMN payment_stack_id TEXT NOT NULL DEFAULT 'local';
ALTER TABLE facilitated_transactions ADD COLUMN is_sandbox BOOLEAN NOT NULL DEFAULT 1;

-- Create index for efficient filtering by payment stack and environment
CREATE INDEX idx_facilitated_transactions_payment_stack
ON facilitated_transactions(payment_stack_id, is_sandbox);
