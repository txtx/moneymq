-- Add payment_hash column to facilitated_transactions
-- This column will store a SHA256 hash of the x402_payment_requirement for efficient lookups
ALTER TABLE facilitated_transactions ADD COLUMN payment_hash TEXT;

-- Create unique index on payment_hash to ensure idempotency
-- This prevents duplicate transaction records for the same payment requirement
CREATE UNIQUE INDEX idx_facilitated_transactions_payment_hash
ON facilitated_transactions(payment_hash);

-- Backfill existing records with computed hashes
-- For existing records without payment_hash, we'll compute it from x402_payment_requirement
-- Note: In production, you may want to do this in batches for large datasets
