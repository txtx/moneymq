-- Drop the unique index on transaction_id
DROP INDEX IF EXISTS idx_facilitated_transactions_transaction_id;

-- Drop the transaction_id column
ALTER TABLE facilitated_transactions DROP COLUMN transaction_id;
