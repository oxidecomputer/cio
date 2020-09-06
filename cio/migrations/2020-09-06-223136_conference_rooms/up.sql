CREATE TABLE conference_rooms (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    typev VARCHAR NOT NULL UNIQUE,
    building VARCHAR NOT NULL,
    capacity INTEGER NOT NULL,
    floor VARCHAR NOT NULL,
    section VARCHAR NOT NULL
)
