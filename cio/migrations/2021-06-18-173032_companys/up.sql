CREATE TABLE companys (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    gsuite_domain VARCHAR NOT NULL,
    github_org VARCHAR NOT NULL,
    website VARCHAR NOT NULL UNIQUE,
    domain VARCHAR NOT NULL,
    gsuite_account_id VARCHAR NOT NULL,
    gsuite_subject VARCHAR NOT NULL,
    phone VARCHAR NOT NULL,
    okta_domain VARCHAR NOT NULL,
    okta_api_key VARCHAR NOT NULL,
    mailchimp_list_id VARCHAR NOT NULL,
    github_app_installation_id INTEGER NOT NULL,
    cloudflare_api_key VARCHAR NOT NULL,
    checkr_api_key VARCHAR NOT NULL,
    printer_url VARCHAR NOT NULL,
    tailscale_api_key VARCHAR NOT NULL,
    tripactions_client_id VARCHAR NOT NULL,
    tripactions_client_secret VARCHAR NOT NULL,
    airtable_api_key VARCHAR NOT NULL,
    airtable_enterprise_account_id VARCHAR NOT NULL,
    airtable_workspace_id VARCHAR NOT NULL,
    airtable_workspace_read_only_id VARCHAR NOT NULL,
    airtable_base_id_customer_leads VARCHAR NOT NULL,
    airtable_base_id_directory VARCHAR NOT NULL,
    airtable_base_id_misc VARCHAR NOT NULL,
    airtable_base_id_roadmap VARCHAR NOT NULL,
    airtable_base_id_hiring VARCHAR NOT NULL,
    airtable_base_id_shipments VARCHAR NOT NULL,
    airtable_base_id_finance VARCHAR NOT NULL,
    airtable_base_id_swag VARCHAR NOT NULL,
    airtable_base_id_assets VARCHAR NOT NULL,
    airtable_base_id_travel VARCHAR NOT NULL,
    airtable_base_id_cio VARCHAR NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
