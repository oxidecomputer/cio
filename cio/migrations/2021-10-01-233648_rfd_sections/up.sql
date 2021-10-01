CREATE TABLE rfd_sections (
    id SERIAL PRIMARY KEY,
    anchor TEXT NOT NULL,
    content TEXT NOT NULL,
    name TEXT NOT NULL,
    rfds_id INTEGER NOT NULL
);

ALTER TABLE "rfd_sections" ADD FOREIGN KEY ("rfds_id") REFERENCES "rfds"("id") ON DELETE CASCADE ON UPDATE CASCADE
