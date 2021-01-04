CREATE TABLE links (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    link VARCHAR NOT NULL,
    aliases TEXT [] NOT NULL,
    short_link VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL
)
