ALTER TABLE users ADD COLUMN materials VARCHAR NOT NULL DEFAULT '';

UPDATE
  users
SET
  materials = applicant.materials
FROM (
  SELECT
    email,
    materials
  FROM
    applicants
) AS applicant
WHERE
  users.recovery_email = applicant.email;