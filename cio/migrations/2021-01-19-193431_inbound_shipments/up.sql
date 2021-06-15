CREATE TABLE inbound_shipments (
    id SERIAL PRIMARY KEY,
    tracking_number VARCHAR NOT NULL UNIQUE,
    carrier VARCHAR NOT NULL,
    tracking_link VARCHAR NOT NULL,
    oxide_tracking_link VARCHAR NOT NULL,
    tracking_status VARCHAR NOT NULL,
    shipped_time TIMESTAMPTZ,
    delivered_time TIMESTAMPTZ,
    eta TIMESTAMPTZ,
    messages VARCHAR NOT NULL,
    order_number VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    notes VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
