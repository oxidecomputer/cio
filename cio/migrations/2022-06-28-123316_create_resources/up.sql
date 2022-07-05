CREATE TABLE resources (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    typev VARCHAR NOT NULL,
    building VARCHAR NOT NULL,
    capacity INTEGER NOT NULL,
    floor VARCHAR NOT NULL,
    section VARCHAR NOT NULL,
    category VARCHAR NOT NULL,
    link_to_building TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL,
    cio_company_id INTEGER NOT NULL
);

INSERT INTO resources (
  id,
  name,
  description,
  typev,
  building,
  capacity,
  floor,
  section,
  category,
  link_to_building,
  airtable_record_id,
  cio_company_id
) SELECT
    id,
    name,
    description,
    typev,
    building,
    capacity,
    floor,
    section,
    'ConferenceRoom',
    link_to_building,
    '',
    cio_company_id
  FROM
    conference_rooms;

ALTER TABLE resources ADD FOREIGN KEY (cio_company_id) REFERENCES companys(id) ON DELETE CASCADE ON UPDATE CASCADE;
