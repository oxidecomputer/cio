CREATE TABLE companys (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    gsuite_domain VARCHAR NOT NULL,
    github_org VARCHAR NOT NULL,
    website VARCHAR NOT NULL UNIQUE,
    domain VARCHAR NOT NULL,
    gsuite_account_id VARCHAR NOT NULL,
    gsuite_subject VARCHAR NOT NULL,
    phone VARCHAR NOT NULL,
    okta_domain VARCHAR NOT NULL,
    mailchimp_list_id VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
