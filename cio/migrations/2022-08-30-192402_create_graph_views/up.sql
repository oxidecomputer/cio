CREATE VIEW graph_applicants AS
  SELECT id, name, role, status, submitted_time, email, value_reflected, value_violated, values_in_tension, start_date, offer_created, offer_completed, interviews_started, interviews_completed, rejection_sent_date_time, cio_company_id FROM applicants;

CREATE VIEW graph_mailing_list_subscribers AS
  SELECT id, date_added, cio_company_id FROM mailing_list_subscribers;

CREATE VIEW graph_outbound_shipments AS
  SELECT id, name, email, created_time, shipped_time, delivered_time, cio_company_id FROM outbound_shipments;

CREATE VIEW graph_page_views AS
  SELECT id, time, domain, path, user_email, cio_company_id FROM page_views;

CREATE VIEW graph_rack_line_subscribers AS
  SELECT id, date_added, cio_company_id FROM rack_line_subscribers;

CREATE VIEW graph_swag_inventory_items AS
  SELECT id, size, current_stock, item FROM swag_inventory_items;

CREATE VIEW graph_users AS
  SELECT id, first_name, last_name, start_date, department FROM users;