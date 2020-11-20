CREATE TABLE certificates (
    id SERIAL PRIMARY KEY,
    domain VARCHAR NOT NULL UNIQUE,
    certificate VARCHAR NOT NULL UNIQUE,
    private_key VARCHAR NOT NULL UNIQUE,
    valid_days_left INTEGER NOT NULL,
    expiration_date DATE NOT NULL
)
