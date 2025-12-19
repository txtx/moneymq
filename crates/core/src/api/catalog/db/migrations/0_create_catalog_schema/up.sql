------------------------------------------------------------
-- products
------------------------------------------------------------
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    -- Reference to the payment stack (subdomain)
    payment_stack_id TEXT NOT NULL,
    -- Product identifier from catalog YAML
    product_id TEXT NOT NULL,
    -- Product name
    name TEXT NOT NULL,
    -- Product description
    description TEXT,
    -- Product type: "service", "good", etc.
    product_type TEXT NOT NULL DEFAULT 'service',
    -- Unit label for metered products
    unit_label TEXT,
    -- Whether the product is active
    active BOOLEAN NOT NULL DEFAULT TRUE,
    -- Product metadata as JSON
    metadata TEXT,
    -- Whether this is sandbox data
    is_sandbox BOOLEAN NOT NULL DEFAULT FALSE,
    -- Unique constraint on payment_stack + product_id + sandbox
    UNIQUE(payment_stack_id, product_id, is_sandbox)
);

------------------------------------------------------------
-- prices
------------------------------------------------------------
CREATE TABLE IF NOT EXISTS prices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    -- Reference to the product
    product_id INTEGER NOT NULL,
    -- Price identifier (optional, for external references)
    price_id TEXT,
    -- Pricing type: "one_time" or "recurring"
    pricing_type TEXT NOT NULL DEFAULT 'one_time',
    -- Currency code (e.g., "usd", "usdc")
    currency TEXT NOT NULL DEFAULT 'usd',
    -- Amount in smallest unit (cents for USD)
    unit_amount BIGINT NOT NULL,
    -- For recurring: interval (day, week, month, year)
    recurring_interval TEXT,
    -- For recurring: interval count (e.g., 2 for every 2 months)
    recurring_interval_count INTEGER,
    -- Whether the price is active
    active BOOLEAN NOT NULL DEFAULT TRUE,
    -- Price metadata as JSON
    metadata TEXT,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

------------------------------------------------------------
-- Indexes for common queries
------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_products_payment_stack ON products(payment_stack_id, is_sandbox);
CREATE INDEX IF NOT EXISTS idx_prices_product ON prices(product_id);
