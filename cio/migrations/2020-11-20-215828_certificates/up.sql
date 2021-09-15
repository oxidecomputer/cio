CREATE TABLE certificates (
    id SERIAL PRIMARY KEY,
    domain VARCHAR NOT NULL UNIQUE,
    certificate TEXT NOT NULL,
    private_key TEXT NOT NULL,
    valid_days_left INTEGER NOT NULL,
    expiration_date DATE NOT NULL,
    repos [] TEXT NOT NULL,
    certificate_github_actions_secret_name VARCHAR NOT NULL,
    private_key_github_actions_secret_name VARCHAR NOT NULL,
    notify_slack_channels [] TEXT NOT NULL,
    airtable_record_id VARCHAR NOT NULL
)
