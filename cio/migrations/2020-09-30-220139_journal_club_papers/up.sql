CREATE TABLE journal_club_papers (
    id SERIAL PRIMARY KEY,
    title VARCHAR NOT NULL UNIQUE,
    link VARCHAR NOT NULL UNIQUE,
    meeting VARCHAR NOT NULL,
    link_to_meeting TEXT [] NOT NULL
)
