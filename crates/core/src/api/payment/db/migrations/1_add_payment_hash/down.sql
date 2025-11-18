-- Remove the unique index
DROP INDEX IF EXISTS idx_facilitated_transactions_payment_hash;

-- Remove the payment_hash column
ALTER TABLE facilitated_transactions DROP COLUMN payment_hash;
