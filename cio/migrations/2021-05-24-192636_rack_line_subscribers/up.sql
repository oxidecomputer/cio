CREATE TABLE rack_line_subscribers (
    id SERIAL PRIMARY KEY,
    email VARCHAR NOT NULL UNIQUE,
    name VARCHAR NOT NULL,
    company VARCHAR NOT NULL,
    company_size VARCHAR NOT NULL,
    interest TEXT NOT NULL,
    date_added TIMESTAMPTZ NOT NULL,
    date_optin TIMESTAMPTZ NOT NULL,
    date_last_changed TIMESTAMPTZ NOT NULL,
    notes TEXT NOT NULL,
    tags TEXT [] NOT NULL,
    link_to_people TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
