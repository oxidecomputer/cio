CREATE TABLE conference_rooms (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    typev VARCHAR NOT NULL,
    building VARCHAR NOT NULL,
    link_to_building TEXT [] NOT NULL,
    capacity INTEGER NOT NULL,
    floor VARCHAR NOT NULL,
    section VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL
)
