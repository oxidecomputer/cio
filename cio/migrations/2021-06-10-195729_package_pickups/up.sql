CREATE TABLE package_pickups (
    id SERIAL PRIMARY KEY,
    shippo_id VARCHAR UNIQUE NOT NULL,
    confirmation_code VARCHAR NOT NULL,
    carrier VARCHAR NOT NULL,
    status VARCHAR NOT NULL,
    location VARCHAR NOT NULL,
	transactions TEXT [] NOT NULL,
	link_to_outbound_shipments TEXT [] NOT NULL,
    requested_start_time TIMESTAMPTZ NOT NULL,
    requested_end_time TIMESTAMPTZ NOT NULL,
    confirmed_start_time TIMESTAMPTZ DEFAULT NULL,
    confirmed_end_time TIMESTAMPTZ DEFAULT NULL,
    cancel_by_time TIMESTAMPTZ DEFAULT NULL,
    messages VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
