CREATE TABLE resources (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    typev VARCHAR NOT NULL,
    building VARCHAR NOT NULL,
    link_to_building TEXT [] NOT NULL,
    capacity INTEGER NOT NULL,
    floor VARCHAR NOT NULL,
    section VARCHAR NOT NULL,
    category VARCHAR NOT NULL,
    cio_company_id INTEGER NOT NULL,
    airtable_record_id VARCHAR NOT NULL
);

ALTER TABLE resources ADD FOREIGN KEY (cio_company_id) REFERENCES companys(id) ON DELETE CASCADE ON UPDATE CASCADE;
