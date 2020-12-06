use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::thread;
use std::time;

use acme_lib::create_p384_key;
use acme_lib::persist::FilePersist;
use acme_lib::{Directory, DirectoryUrl};
use async_trait::async_trait;
use chrono::NaiveDate;
use chrono::{DateTime, TimeZone, Utc};
use cloudflare::endpoints::{dns, zone};
use cloudflare::framework::{
    async_api::{ApiClient, Client},
    auth::Credentials,
    Environment, HttpApiClientConfig,
};
use hubcaps::Github;
use macros::db_struct;
use openssl::x509::X509;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_BASE_ID_MISC, AIRTABLE_CERTIFICATES_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::schema::certificates;
use crate::utils::github_org;

/// Creates a Let's Encrypt SSL certificate for a domain by using a DNS challenge.
/// The DNS Challenge TXT record is added to Cloudflare automatically.
pub async fn create_ssl_certificate(domain: &str) -> NewCertificate {
    let email = env::var("CLOUDFLARE_EMAIL").unwrap();

    // Create the Cloudflare client.
    let cf_creds = Credentials::UserAuthKey {
        email: env::var("CLOUDFLARE_EMAIL").unwrap(),
        key: env::var("CLOUDFLARE_TOKEN").unwrap(),
    };
    let api_client = Client::new(cf_creds, HttpApiClientConfig::default(), Environment::Production).unwrap();

    // Save/load keys and certificates to a temporary directory, we will re-save elsewhere.
    let persist = FilePersist::new(env::temp_dir());

    // Create a directory entrypoint.
    // Use DirectoryUrl::LetsEncrypStaging for dev/testing.
    let dir = Directory::from_url(persist, DirectoryUrl::LetsEncrypt).unwrap();

    // Reads the private account key from persistence, or
    // creates a new one before accessing the API to establish
    // that it's there.
    let acc = dir.account(&email).unwrap();

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
        let domain_parts: Vec<&str> = domain.split(".").collect();
        let root_domain = format!("{}.{}", domain_parts[domain_parts.len() - 2], domain_parts[domain_parts.len() - 1]);

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
                zone_identifier: &zone_identifier,
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
                    zone_identifier: &zone_identifier,
                    params: dns::CreateDnsRecordParams {
                        name: &record_name,
                        content: dns::DnsContent::TXT { content: challenge.dns_proof() },
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
                    zone_identifier: &zone_identifier,
                    identifier: &dns_records[0].id,
                    params: dns::UpdateDnsRecordParams {
                        name: &record_name,
                        content: dns::DnsContent::TXT { content: challenge.dns_proof() },
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
    }
}

/// A data type to hold the values of a let's encrypt certificate for a domain.
#[db_struct {
    new_name = "Certificate",
    base_id = "AIRTABLE_BASE_ID_MISC",
    table = "AIRTABLE_CERTIFICATES_TABLE",
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
    #[serde(default = "crate::utils::default_date", serialize_with = "crate::configs::null_date_format::serialize")]
    pub expiration_date: NaiveDate,
}

impl NewCertificate {
    /// For a certificate struct, populate the certificate fields for the domain.
    /// This will create the cert from Let's Encrypt and update Cloudflare TXT records for the
    /// verification.
    pub async fn populate(&mut self) {
        *self = create_ssl_certificate(&self.domain).await;
    }

    /// For a certificate struct, populate the certificate and private_key fields from
    /// GitHub, then fill in the rest.
    pub async fn populate_from_github(&mut self, github: &Github) {
        let repo = github.repo(github_org(), "configs");
        let cert = repo
            .content()
            .file(&format!("nginx/ssl/{}/fullchain.pem", self.domain.replace("*.", "wildcard.")), "master")
            .await
            .unwrap();
        let priv_key = repo
            .content()
            .file(&format!("nginx/ssl/{}/privkey.pem", self.domain.replace("*.", "wildcard.")), "master")
            .await
            .unwrap();

        self.certificate = from_utf8(&cert.content).unwrap().to_string();
        self.private_key = from_utf8(&priv_key.content).unwrap().to_string();
        let exp_date = self.expiration_date();
        self.expiration_date = exp_date.date().naive_utc();
        self.valid_days_left = self.valid_days_left();
    }

    /// For a certificate struct, populate the certificate and private_key fields from
    /// disk, then fill in the rest.
    pub fn populate_from_disk(&mut self, dir: &str) {
        let path = self.get_path(dir);

        self.certificate = fs::read_to_string(path.clone().join("fullchain.pem")).unwrap_or("".to_string());
        self.private_key = fs::read_to_string(path.clone().join("privkey.pem")).unwrap_or("".to_string());

        if !self.certificate.is_empty() {
            let exp_date = self.expiration_date();
            self.expiration_date = exp_date.date().naive_utc();
            self.valid_days_left = self.valid_days_left();
        }
    }

    fn get_path(&self, dir: &str) -> PathBuf {
        Path::new(dir).join(self.domain.replace("*.", "wildcard.").to_string())
    }

    /// Saves the fullchain certificate and privkey to /{dir}/{domain}/{privkey.pem,fullchain.pem}
    pub fn save_to_directory(&self, dir: &str) {
        let path = self.get_path(dir);

        // Create the directory if it does not exist.
        fs::create_dir_all(path.clone()).unwrap();

        // Write the files.
        fs::write(path.clone().join("fullchain.pem"), self.certificate.as_bytes()).unwrap();
        fs::write(path.join("privkey.pem"), self.private_key.as_bytes()).unwrap();
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
        // load as x509
        let x509 = X509::from_pem(self.certificate.as_bytes()).expect("from_pem");

        // convert asn1 time to Tm
        let not_after = format!("{}", x509.not_after());
        // Display trait produces this format, which is kinda dumb.
        // Apr 19 08:48:46 2019 GMT
        Utc.datetime_from_str(&not_after, "%h %e %H:%M:%S %Y %Z").expect("strptime")
    }
}

/// Implement updating the Airtable record for a Certificate.
#[async_trait]
impl UpdateAirtableRecord<Certificate> for Certificate {
    async fn update_airtable_record(&mut self, _record: Certificate) {}
}
