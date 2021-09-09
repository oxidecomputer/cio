CREATE TABLE functions (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    status VARCHAR NOT NULL,
    conclusion VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ DEFAULT NULL,
    logs TEXT NOT NULL,
    cio_company_id INTEGER NOT NULL DEFAULT 0,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
