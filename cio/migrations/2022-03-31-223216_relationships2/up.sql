-- Your SQL goes here
ALTER TABLE api_tokens ADD FOREIGN KEY (auth_company_id) REFERENCES companys(id) ON DELETE CASCADE ON UPDATE CASCADE;
