DROP INDEX idx_sf_new_leads;

ALTER TABLE rack_line_subscribers DROP COLUMN sf_lead_exclude;
ALTER TABLE rack_line_subscribers DROP COLUMN sf_lead_id;