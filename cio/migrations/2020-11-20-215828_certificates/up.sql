CREATE TABLE certificates (
    id SERIAL PRIMARY KEY,
    domain VARCHAR NOT NULL UNIQUE,
    certificate TEXT NOT NULL,
    private_key TEXT NOT NULL,
    valid_days_left INTEGER NOT NULL,
    expiration_date DATE NOT NULL
)
