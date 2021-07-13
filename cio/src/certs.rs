#![allow(clippy::from_over_into)]
use std::{
    env, fs,
    path::{Path, PathBuf},
    str::from_utf8,
    thread, time,
};

use acme_lib::{create_p384_key, persist::FilePersist, Directory, DirectoryUrl};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use cloudflare::{
    endpoints::{dns, zone},
    framework::async_api::ApiClient,
};
use macros::db;
use openssl::x509::X509;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_CERTIFICATES_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    schema::certificates,
    utils::{create_or_update_file_in_github_repo, get_file_content_from_repo},
};

/// Creates a Let's Encrypt SSL certificate for a domain by using a DNS challenge.
/// The DNS Challenge TXT record is added to Cloudflare automatically.
pub async fn create_ssl_certificate(domain: &str, company: &Company) -> NewCertificate {
    let api_client = company.authenticate_cloudflare().unwrap();

    // Save/load keys and certificates to a temporary directory, we will re-save elsewhere.
    let persist = FilePersist::new(env::temp_dir());

    // Create a directory entrypoint.
    // Use DirectoryUrl::LetsEncrypStaging for dev/testing.
    let dir = Directory::from_url(persist, DirectoryUrl::LetsEncrypt).unwrap();

    // Reads the private account key from persistence, or
    // creates a new one before accessing the API to establish
    // that it's there.
    let acc = dir.account(&company.gsuite_subject).unwrap();

    // Order a new TLS certificate for a domain.
    let mut ord_new = acc.new_order(domain, &[]).unwrap();

    // If the ownership of the domain(s) have already been
    // authorized in a previous order, you might be able to
    // skip validation. The ACME API provider decides.
    let ord_csr = loop {
        // are we done?
        if let Some(ord_csr) = ord_new.confirm_validations() {
            break ord_csr;
        }

        // Get the possible authorizations (for a single domain
        // this will only be one element).
        let auths = ord_new.authorizations().unwrap();

        // Get the proff we need for the TXT record:
        // _acme-challenge.<domain-to-be-proven>.  TXT  <proof>
        let challenge = auths[0].dns_challenge();

        // Create a TXT record for _acme-challenge.{domain} with the value of
        // the proof.
        // Use the Cloudflare API for this.

        // We need the root of the domain not a subdomain.
        let domain_parts: Vec<&str> = domain.split('.').collect();
        let root_domain = format!(
            "{}.{}",
            domain_parts[domain_parts.len() - 2],
            domain_parts[domain_parts.len() - 1]
        );

        // Get the zone ID for the domain.
        let zones = api_client
            .request(&zone::ListZones {
                params: zone::ListZonesParams {
                    name: Some(root_domain.to_string()),
                    ..Default::default()
                },
            })
            .await
            .unwrap()
            .result;

        // Our zone identifier should be the first record's ID.
        let zone_identifier = &zones[0].id;
        let record_name = format!("_acme-challenge.{}", domain.replace("*.", ""));

        // Check if we already have a TXT record and we need to update it.
        let dns_records = api_client
            .request(&dns::ListDnsRecords {
                zone_identifier,
                params: dns::ListDnsRecordsParams {
                    name: Some(record_name.to_string()),
                    ..Default::default()
                },
            })
            .await
            .unwrap()
            .result;

        // If we have a dns record already, update it. If not, create it.
        if dns_records.is_empty() {
            // Create the DNS record.
            let dns_record = api_client
                .request(&dns::CreateDnsRecord {
                    zone_identifier,
                    params: dns::CreateDnsRecordParams {
                        name: &record_name,
                        content: dns::DnsContent::TXT {
                            content: challenge.dns_proof(),
                        },
                        ttl: None,
                        proxied: None,
                        priority: None,
                    },
                })
                .await
                .unwrap()
                .result;

            println!("[certs] created dns record: {:?}", dns_record);
        } else {
            // Update the DNS record.
            let dns_record = api_client
                .request(&dns::UpdateDnsRecord {
                    zone_identifier,
                    identifier: &dns_records[0].id,
                    params: dns::UpdateDnsRecordParams {
                        name: &record_name,
                        content: dns::DnsContent::TXT {
                            content: challenge.dns_proof(),
                        },
                        ttl: None,
                        proxied: None,
                    },
                })
                .await
                .unwrap()
                .result;

            println!("[certs] updated dns record: {:?}", dns_record);
        }

        // TODO: make this less awful than a sleep.
        println!("validating the proof...");
        let dur = time::Duration::from_secs(10);
        thread::sleep(dur);

        // After the TXT record is accessible, the calls
        // this to tell the ACME API to start checking the
        // existence of the proof.
        //
        // The order at ACME will change status to either
        // confirm ownership of the domain, or fail due to the
        // not finding the proof. To see the change, we poll
        // the API with 5000 milliseconds wait between.
        challenge.validate(5000).unwrap();

        // Update the state against the ACME API.
        ord_new.refresh().unwrap();
    };

    // Ownership is proven. Create a private key for
    // the certificate. These are provided for convenience, you
    // can provide your own keypair instead if you want.
    let pkey_pri = create_p384_key();

    // Submit the CSR. This causes the ACME provider to enter a
    // state of "processing" that must be polled until the
    // certificate is either issued or rejected. Again we poll
    // for the status change.
    let ord_cert = ord_csr.finalize_pkey(pkey_pri, 5000).unwrap();

    // Now download the certificate. Also stores the cert in
    // the persistence.
    let cert = ord_cert.download_and_save_cert().unwrap();

    NewCertificate {
        private_key: cert.private_key().to_string(),
        certificate: cert.certificate().to_string(),
        domain: domain.to_string(),
        valid_days_left: cert.valid_days_left() as i32,
        expiration_date: crate::utils::default_date(),
        cio_company_id: company.id,
    }
}

/// A data type to hold the values of a let's encrypt certificate for a domain.
#[db {
    new_struct_name = "Certificate",
    airtable_base = "misc",
    airtable_table = "AIRTABLE_CERTIFICATES_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "domain" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "certificates"]
pub struct NewCertificate {
    pub domain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certificate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub private_key: String,
    #[serde(default)]
    pub valid_days_left: i32,
    #[serde(
        default = "crate::utils::default_date",
        serialize_with = "crate::configs::null_date_format::serialize"
    )]
    pub expiration_date: NaiveDate,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl NewCertificate {
    /// For a certificate struct, populate the certificate fields for the domain.
    /// This will create the cert from Let's Encrypt and update Cloudflare TXT records for the
    /// verification.
    pub async fn populate(&mut self, company: &Company) {
        *self = create_ssl_certificate(&self.domain, company).await;
    }

    /// For a certificate struct, populate the certificate and private_key fields from
    /// GitHub, then fill in the rest.
    pub async fn populate_from_github(&mut self, github: &octorust::Client, company: &Company) {
        let owner = &company.github_org;
        let repo = "configs";
        let r = github.repos().get(owner, repo).await.unwrap();

        let (cert, _) = get_file_content_from_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("fullchain.pem"),
        )
        .await
        .unwrap();
        if !cert.is_empty() {
            self.certificate = from_utf8(&cert).unwrap().to_string();
        }

        let (p, _) = get_file_content_from_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("privkey.pem"),
        )
        .await
        .unwrap();
        if !p.is_empty() {
            self.private_key = from_utf8(&p).unwrap().to_string();
        }

        let exp_date = self.expiration_date();
        self.expiration_date = exp_date.date().naive_utc();
        self.valid_days_left = self.valid_days_left();
    }

    /// For a certificate struct, populate the certificate and private_key fields from
    /// disk, then fill in the rest.
    pub fn populate_from_disk(&mut self, dir: &str) {
        let path = self.get_path(dir);

        self.certificate = fs::read_to_string(path.join("fullchain.pem")).unwrap_or_default();
        self.private_key = fs::read_to_string(path.join("privkey.pem")).unwrap_or_default();

        if !self.certificate.is_empty() {
            let exp_date = self.expiration_date();
            self.expiration_date = exp_date.date().naive_utc();
            self.valid_days_left = self.valid_days_left();
        }
    }

    fn get_path(&self, dir: &str) -> PathBuf {
        Path::new(dir).join(self.domain.replace("*.", "wildcard."))
    }

    fn get_github_path(&self, file: &str) -> String {
        format!(
            "/nginx/ssl/{}/{}",
            self.domain.replace("*.", "wildcard."),
            file
        )
    }

    /// Saves the fullchain certificate and privkey to /{dir}/{domain}/{privkey.pem,fullchain.pem}
    pub fn save_to_directory(&self, dir: &str) {
        if self.certificate.is_empty() {
            // Return early.
            return;
        }

        let path = self.get_path(dir);

        // Create the directory if it does not exist.
        fs::create_dir_all(path.clone()).unwrap();

        // Write the files.
        fs::write(path.join("fullchain.pem"), self.certificate.as_bytes()).unwrap();
        fs::write(path.join("privkey.pem"), self.private_key.as_bytes()).unwrap();
    }

    /// Saves the fullchain certificate and privkey to the configs github repo.
    pub async fn save_to_github_repo(&self, github: &octorust::Client, company: &Company) {
        if self.certificate.is_empty() {
            // Return early.
            return;
        }

        let owner = &company.github_org;
        let repo = "configs";
        let r = github.repos().get(owner, repo).await.unwrap();

        // Write the files.
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("fullchain.pem"),
            self.certificate.as_bytes().to_vec(),
        )
        .await;
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("privkey.pem"),
            self.private_key.as_bytes().to_vec(),
        )
        .await;
    }

    /// Inspect the certificate to count the number of (whole) valid days left.
    ///
    /// It's up to the ACME API provider to decide how long an issued certificate is valid.
    /// Let's Encrypt sets the validity to 90 days. This function reports 89 days for newly
    /// issued cert, since it counts _whole_ days.
    ///
    /// It is possible to get negative days for an expired certificate.
    pub fn valid_days_left(&self) -> i32 {
        let expires = self.expiration_date();
        let dur = expires - Utc::now();

        dur.num_days() as i32
    }

    /// Inspect the certificate to get the expiration_date.
    pub fn expiration_date(&self) -> DateTime<Utc> {
        if self.certificate.is_empty() {
            return Utc::now();
        }

        // load as x509
        let x509 = X509::from_pem(self.certificate.as_bytes()).expect("from_pem");

        // convert asn1 time to Tm
        let not_after = format!("{}", x509.not_after());
        // Display trait produces this format, which is kinda dumb.
        // Apr 19 08:48:46 2019 GMT
        Utc.datetime_from_str(&not_after, "%h %e %H:%M:%S %Y %Z")
            .expect("strptime")
    }
}

/// Implement updating the Airtable record for a Certificate.
#[async_trait]
impl UpdateAirtableRecord<Certificate> for Certificate {
    async fn update_airtable_record(&mut self, _record: Certificate) {}
}
