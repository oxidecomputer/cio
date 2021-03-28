CREATE TABLE groups (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    link VARCHAR NOT NULL,
    aliases TEXT [] NOT NULL,
    members TEXT [] NOT NULL,
    allow_external_members BOOLEAN NOT NULL DEFAULT 'f',
    allow_web_posting BOOLEAN NOT NULL DEFAULT 'f',
    is_archived BOOLEAN NOT NULL DEFAULT 'f',
    who_can_discover_group VARCHAR NOT NULL,
    who_can_join VARCHAR NOT NULL,
    who_can_moderate_members VARCHAR NOT NULL,
    who_can_post_message VARCHAR NOT NULL,
    who_can_view_group VARCHAR NOT NULL,
    who_can_view_membership VARCHAR NOT NULL,
    enable_collaborative_inbox BOOLEAN NOT NULL DEFAULT 'f',
    airtable_record_id VARCHAR NOT NULL
)
