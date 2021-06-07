CREATE TABLE accounts_payables (
    id SERIAL PRIMARY KEY,
    confirmation_number VARCHAR NOT NULL UNIQUE,
    amount REAL NOT NULL DEFAULT 0,
    invoice_number VARCHAR NOT NULL,
    vendor VARCHAR NOT NULL,
    currency VARCHAR NOT NULL,
    date DATE NOT NULL,
    payment_type VARCHAR NOT NULL,
    status VARCHAR NOT NULL,
    invoices TEXT [] NOT NULL,
    link_to_vendor TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
