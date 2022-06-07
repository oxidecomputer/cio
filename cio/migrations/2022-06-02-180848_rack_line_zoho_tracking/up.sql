ALTER TABLE rack_line_subscribers ADD COLUMN zoho_lead_id VARCHAR NOT NULL DEFAULT '';
ALTER TABLE rack_line_subscribers ADD COLUMN zoho_lead_exclude BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_zoho_new_leads ON rack_line_subscribers(zoho_lead_id, zoho_lead_exclude, date_added);