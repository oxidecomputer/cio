CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    first_name VARCHAR NOT NULL,
    last_name VARCHAR NOT NULL,
    username VARCHAR NOT NULL UNIQUE,
    aliases TEXT [] NOT NULL,
    recovery_email VARCHAR NOT NULL,
    recovery_phone VARCHAR NOT NULL,
    gender VARCHAR NOT NULL,
    chat VARCHAR NOT NULL,
    github VARCHAR NOT NULL,
    twitter VARCHAR NOT NULL,
    groups TEXT [] NOT NULL,
    is_super_admin BOOLEAN NOT NULL DEFAULT 'f',
    building VARCHAR NOT NULL
)
