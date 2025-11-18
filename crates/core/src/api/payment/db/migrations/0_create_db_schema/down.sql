-- Drop triggers
DROP TRIGGER IF EXISTS update_facilitated_transactions_updated_at ON facilitated_transactions;
DROP TRIGGER IF EXISTS update_transaction_customers_updated_at ON transaction_customers;

-- Drop timestamp function
DROP FUNCTION IF EXISTS set_timestamp_update();

-- Drop tables (order matters due to FK)
DROP TABLE IF EXISTS facilitated_transactions;
DROP TABLE IF EXISTS transaction_customers;
