DROP INDEX idx_zoho_new_leads;

ALTER TABLE rack_line_subscribers DROP COLUMN zoho_lead_exclude;
ALTER TABLE rack_line_subscribers DROP COLUMN zoho_lead_id;