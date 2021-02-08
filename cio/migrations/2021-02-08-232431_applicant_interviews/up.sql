CREATE TABLE applicant_interviews (
    id SERIAL PRIMARY KEY,
	start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
	name VARCHAR NOT NULL,
	email VARCHAR NOT NULL,
	interviewers TEXT [] NOT NULL,
	google_event_id VARCHAR NOT NULL,
	event_link VARCHAR NOT NULL,
	link_to_applicant TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
