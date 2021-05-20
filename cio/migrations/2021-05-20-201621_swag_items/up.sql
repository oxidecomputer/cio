CREATE TABLE swag_items (
    id SERIAL PRIMARY KEY,
    name VARCHAR UNIQUE NOT NULL,
    description VARCHAR NOT NULL DEFAULT '',
    image VARCHAR NOT NULL,
    link_to_inventory TEXT [] NOT NULL,
    link_to_barcode_scans TEXT [] NOT NULL,
    link_to_order_january_2020 TEXT [] NOT NULL,
    link_to_order_october_2020 TEXT [] NOT NULL,
    link_to_order_may_2021 TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
