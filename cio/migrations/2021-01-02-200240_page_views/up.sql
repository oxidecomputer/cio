CREATE TABLE page_views (
    id SERIAL PRIMARY KEY,
    time TIMESTAMPTZ NOT NULL,
    domain VARCHAR NOT NULL,
    path VARCHAR NOT NULL,
    user_email VARCHAR NOT NULL,
    page_link VARCHAR NOT NULL,
    link_to_auth_user TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL
)
