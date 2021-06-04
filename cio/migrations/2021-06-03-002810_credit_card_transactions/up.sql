CREATE TABLE credit_card_transactions (
    id SERIAL PRIMARY KEY,
    transaction_id VARCHAR NOT NULL UNIQUE,
    card_vendor VARCHAR NOT NULL,
    amount REAL NOT NULL DEFAULT 0,
    employee_email VARCHAR NOT NULL,
    card_id VARCHAR NOT NULL,
    merchant_id VARCHAR NOT NULL,
    merchant_name VARCHAR NOT NULL,
    category_id INTEGER DEFAULT 0 NOT NULL,
    category_name VARCHAR NOT NULL,
    state VARCHAR NOT NULL,
    time TIMESTAMPTZ NOT NULL,
    receipts TEXT [] NOT NULL,
    link_to_vendor TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
