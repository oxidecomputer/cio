CREATE TABLE api_tokens (
    id SERIAL PRIMARY KEY,
    product VARCHAR NOT NULL,
    company_id VARCHAR NOT NULL UNIQUE,
    token_type VARCHAR NOT NULL,
    access_token VARCHAR NOT NULL,
    expires_in INTEGER DEFAULT 0 NOT NULL,
    refresh_token VARCHAR NOT NULL,
    refresh_token_expires_in INTEGER DEFAULT 0 NOT NULL,
    expires_date TIMESTAMPTZ DEFAULT NULL,
    refresh_token_expires_date TIMESTAMPTZ DEFAULT NULL,
    endpoint VARCHAR NOT NULL,
    last_updated_at TIMESTAMPTZ NOT NULL,
    cio_company_id INTEGER NOT NULL,
    company [] TEXT NOT NULL,
    auth_company_id INTEGER NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
