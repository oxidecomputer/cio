CREATE TABLE rfd_sections (
    id SERIAL PRIMARY KEY,
    anchor TEXT NOT NULL,
    content TEXT NOT NULL,
    name TEXT NOT NULL,
    rfds_id INTEGER NOT NULL
)
