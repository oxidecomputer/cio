CREATE TABLE upload_tokens (
    id SERIAL PRIMARY KEy,
    email VARCHAR NOT NULL,
    token VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_upload_token_get ON upload_tokens(email, expires_at, used_at);
CREATE INDEX IF NOT EXISTS idx_upload_token_test ON upload_tokens(email, token, expires_at, used_at);