-- Remove payment stack context columns
-- Note: SQLite doesn't support DROP COLUMN directly, so we need to recreate the table

DROP INDEX IF EXISTS idx_facilitated_transactions_payment_stack;

-- SQLite workaround: create new table without the columns, copy data, drop old, rename
CREATE TABLE facilitated_transactions_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    product     TEXT,
    customer_id INTEGER,
    amount      TEXT NOT NULL,
    currency    TEXT,
    status      TEXT,
    signature   TEXT,
    x402_payment_requirement TEXT NOT NULL,
    x402_verify_request      TEXT,
    x402_verify_response     TEXT,
    x402_settle_request      TEXT,
    x402_settle_response     TEXT,
    payment_hash             TEXT,
    FOREIGN KEY (customer_id) REFERENCES transaction_customers(id)
);

INSERT INTO facilitated_transactions_new
SELECT id, created_at, updated_at, product, customer_id, amount, currency, status, signature,
       x402_payment_requirement, x402_verify_request, x402_verify_response,
       x402_settle_request, x402_settle_response, payment_hash
FROM facilitated_transactions;

DROP TABLE facilitated_transactions;
ALTER TABLE facilitated_transactions_new RENAME TO facilitated_transactions;

-- Recreate the payment_hash index
CREATE UNIQUE INDEX idx_facilitated_transactions_payment_hash
ON facilitated_transactions(payment_hash);
