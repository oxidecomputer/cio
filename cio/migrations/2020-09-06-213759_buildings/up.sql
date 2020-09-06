CREATE TABLE buildings (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    address VARCHAR NOT NULL,
    city VARCHAR NOT NULL,
    state VARCHAR NOT NULL,
    zipcode VARCHAR NOT NULL,
    country VARCHAR NOT NULL,
    floors TEXT [] NOT NULL
)
