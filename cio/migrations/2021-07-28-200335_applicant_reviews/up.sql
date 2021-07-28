CREATE TABLE applicant_reviews (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    value_reflected VARCHAR NOT NULL,
    value_violated VARCHAR NOT NULL,
    values_in_tension TEXT [] NOT NULL,
    evaluation VARCHAR NOT NULL,
    rationale TEXT [] NOT NULL,
    notes VARCHAR NOT NULL,
    reviewer VARCHAR NOT NULL,
    applicant TEXT [] NOT NULL,
    cio_company_id INTEGER NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
