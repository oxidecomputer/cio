-- Your SQL goes here
ALTER TABLE api_tokens ALTER COLUMN access_token type TEXT;
ALTER TABLE api_tokens ALTER COLUMN refresh_token type TEXT;
