CREATE TABLE barcode_scans (
    id SERIAL PRIMARY KEY,
    time TIMESTAMPTZ NOT NULL,
    name VARCHAR NOT NULL,
    size VARCHAR NOT NULL DEFAULT 'N/A',
    item VARCHAR NOT NULL,
    barcode VARCHAR NOT NULL,
    link_to_item TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
