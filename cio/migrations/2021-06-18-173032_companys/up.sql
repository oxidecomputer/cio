CREATE TABLE companys (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    gsuite_domain VARCHAR NOT NULL,
    github_org VARCHAR NOT NULL,
    website VARCHAR NOT NULL UNIQUE,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
