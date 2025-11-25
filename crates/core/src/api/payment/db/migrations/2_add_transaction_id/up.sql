-- Add transaction_id column to facilitated_transactions
-- This column will store a unique UUID to link verify and settle operations
ALTER TABLE facilitated_transactions ADD COLUMN transaction_id TEXT;

-- Create unique index on transaction_id for fast lookups and uniqueness enforcement
CREATE UNIQUE INDEX idx_facilitated_transactions_transaction_id
ON facilitated_transactions(transaction_id)
WHERE transaction_id IS NOT NULL;
