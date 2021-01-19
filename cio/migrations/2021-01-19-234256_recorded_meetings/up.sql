CREATE TABLE recorded_meetings (
    id SERIAL PRIMARY KEY,
	name VARCHAR NOT NULL,
	description VARCHAR NOT NULL,
	start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
	video VARCHAR NOT NULL,
	chat_log VARCHAR NOT NULL,
    is_recurring BOOLEAN NOT NULL DEFAULT 'f',
	attendees TEXT NOT NULL,
	transcript TEXT NOT NULL,
	google_event_id VARCHAR NOT NULL,
	event_link VARCHAR NOT NULL,
	location VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
