CREATE TABLE journal_club_meetings (
    id SERIAL PRIMARY KEY,
    title VARCHAR NOT NULL UNIQUE,
    issue VARCHAR NOT NULL UNIQUE,
    papers TEXT [] NOT NULL,
    issue_date DATE NOT NULL,
    meeting_date DATE NOT NULL,
    coordinator VARCHAR NOT NULL,
    state VARCHAR NOT NULL,
    recording VARCHAR NOT NULL
)
