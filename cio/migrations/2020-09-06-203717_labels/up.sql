CREATE TABLE github_labels (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    color VARCHAR NOT NULL
)
