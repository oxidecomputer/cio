-- Your SQL goes here
CREATE INDEX IF NOT EXISTS idx_applicant_reviews ON applicant_reviews(name);
CREATE INDEX IF NOT EXISTS idx_applicants ON applicants(email,sheet_id);
CREATE INDEX IF NOT EXISTS idx_applicants_email ON applicants(email);
CREATE INDEX IF NOT EXISTS idx_applicant_reviewers ON applicant_reviewers(email);
CREATE INDEX IF NOT EXISTS idx_functions ON functions(saga_id);
CREATE INDEX IF NOT EXISTS idx_functions_cron_lookup ON functions(name, status, created_at);
CREATE INDEX IF NOT EXISTS idx_users ON users(cio_company_id, username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
