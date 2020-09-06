CREATE TABLE links (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    description VARCHAR NOT NULL,
    link VARCHAR NOT NULL UNIQUE,
    aliases TEXT [] NOT NULL
)
