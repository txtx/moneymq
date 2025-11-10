
------------------------------------------------------------
-- transaction_customers
------------------------------------------------------------
CREATE TABLE IF NOT EXISTS transaction_customers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    label       TEXT,
    address     TEXT NOT NULL UNIQUE
);

------------------------------------------------------------
-- facilitated_transactions
------------------------------------------------------------
CREATE TABLE facilitated_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    product     TEXT,
    customer_id INTEGER,
    amount      TEXT NOT NULL,
    currency    TEXT,
    status      TEXT,
    signature                               TEXT,
    x402_payment_requirement          TEXT NOT NULL,
    x402_verify_request                     TEXT,
    x402_verify_response                    TEXT,
    x402_settle_request                     TEXT,
    x402_settle_response                    TEXT,
    FOREIGN KEY (customer_id) REFERENCES transaction_customers(id)
);

------------------------------------------------------------
-- Trigger: update updated_at on UPDATE
------------------------------------------------------------

-- -- Create function
-- CREATE OR REPLACE FUNCTION set_timestamp_update()
-- RETURNS TRIGGER AS $$
-- BEGIN
--     NEW.updated_at = EXTRACT(EPOCH FROM NOW())::BIGINT;
--     RETURN NEW;
-- END;
-- $$ LANGUAGE plpgsql;

-- -- Apply to both tables
-- CREATE TRIGGER update_transaction_customers_updated_at
-- BEFORE UPDATE ON transaction_customers
-- FOR EACH ROW
-- EXECUTE FUNCTION set_timestamp_update();

-- CREATE TRIGGER update_facilitated_transactions_updated_at
-- BEFORE UPDATE ON facilitated_transactions
-- FOR EACH ROW
-- EXECUTE FUNCTION set_timestamp_update();
