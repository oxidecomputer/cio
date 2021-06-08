CREATE TABLE api_tokens (
    id SERIAL PRIMARY KEY,
    product VARCHAR NOT NULL UNIQUE,
    company_id VARCHAR NOT NULL UNIQUE,
    token_type VARCHAR NOT NULL,
    access_token VARCHAR NOT NULL,
    expires_in INTEGER DEFAULT 0 NOT NULL,
    refresh_token VARCHAR NOT NULL,
    refresh_token_expires_in INTEGER DEFAULT 0 NOT NULL,
    last_updated_at TIMESTAMPTZ NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
