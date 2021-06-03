CREATE TABLE credit_card_transactions (
    id SERIAL PRIMARY KEY,
    ramp_id VARCHAR NOT NULL UNIQUE,
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
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
