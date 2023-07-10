ALTER TABLE rack_line_subscribers ADD COLUMN sf_lead_id VARCHAR NOT NULL DEFAULT '';
ALTER TABLE rack_line_subscribers ADD COLUMN sf_lead_exclude BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_sf_new_leads ON rack_line_subscribers(sf_lead_id, sf_lead_exclude, date_added);