#![allow(clippy::from_over_into)]
use std::{env, time};

use acme_lib::{create_p384_key, persist::FilePersist, Certificate as AcmeCertificate, Directory, DirectoryUrl};
use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use google_storage1::{
    api::{Object, Storage},
    hyper,
    hyper::client::connect::Connection,
    hyper::Uri,
};
use macros::db;
use mime::Mime;
use octorust::types::FullRepository;
use openssl::x509::X509;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    airtable::AIRTABLE_CERTIFICATES_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    dns_providers::{DNSProviderOps, DnsRecord, DnsRecordType, DnsUpdateMode},
    schema::certificates,
    utils::{create_or_update_file_in_github_repo, get_file_content_from_repo},
};

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
#[diesel(table_name = certificates)]
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

    /// The repos that use this as a secret for GitHub actions.
    /// The repo must be in the org for this company.
    /// When the certificate is up for renewal it will also update these secrets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,

    /// The name of the secret for the certificate if it is used in GitHub Actions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certificate_github_actions_secret_name: String,

    /// The name of the secret for the private_key if it is used in GitHub Actions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub private_key_github_actions_secret_name: String,

    /// The names of the Slack channels to notify on update.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notify_slack_channels: Vec<String>,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,

    // Subject alternative names to append to the certificate
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sans: Vec<String>,
}

impl NewCertificate {
    /// Creates a Let's Encrypt SSL certificate for a domain by using a DNS challenge.
    /// The DNS Challenge TXT record is added to Cloudflare automatically.
    pub async fn create_cert(&mut self, company: &Company) -> Result<AcmeCertificate> {
        let api_client = company.authenticate_dns_providers().await?;

        // Save/load keys and certificates to a temporary directory, we will re-save elsewhere.
        let persist = FilePersist::new(env::temp_dir());

        // Create a directory entrypoint.
        // Use DirectoryUrl::LetsEncrypStaging for dev/testing.
        let dir = Directory::from_url(persist, DirectoryUrl::LetsEncrypt)?;

        // Reads the private account key from persistence, or
        // creates a new one before accessing the API to establish
        // that it's there.
        let acc = dir.account(&company.gsuite_subject)?;

        log::info!("Authenticated with cert provider");

        // Order a new TLS certificate for a domain.
        let mut ord_new = acc.new_order(&self.domain, &self.sans.iter().map(|s| s.as_str()).collect::<Vec<_>>())?;

        log::info!("Created new cert order for {}", self.domain);

        // If the ownership of the domain(s) have already been
        // authorized in a previous order, you might be able to
        // skip validation. The ACME API provider decides.
        let ord_csr = loop {
            // are we done?
            if let Some(ord_csr) = ord_new.confirm_validations() {
                log::info!("Cert order validated for {}", self.domain);
                break ord_csr;
            }

            // Get the possible authorizations (for a single domain
            // this will only be one element).
            let auths = ord_new.authorizations()?;

            // Get the proff we need for the TXT record:
            // _acme-challenge.<domain-to-be-proven>.  TXT  <proof>
            let challenge = auths[0].dns_challenge();

            log::info!("Retrieved acme challenge for {}", self.domain);

            // Create a TXT record for _acme-challenge.{domain} with the value of
            // the proof.
            // Use the Cloudflare API for this.
            let record_name = format!("_acme-challenge.{}", &self.domain.replace("*.", ""));

            // Ensure our DNS record exists.
            api_client
                .ensure_record(
                    DnsRecord {
                        name: record_name.to_string(),
                        type_: DnsRecordType::TXT,
                        content: challenge.dns_proof(),
                    },
                    DnsUpdateMode::Replace,
                )
                .await?;

            log::info!(
                "Created _acme-challenge record for {}. Sleeping before starting validation",
                self.domain
            );

            // TODO: make this less awful than a sleep.
            let dur = time::Duration::from_secs(10);
            tokio::time::sleep(dur).await;

            log::info!("Waiting for validation for {} to complete", self.domain);

            // After the TXT record is accessible, the calls
            // this to tell the ACME API to start checking the
            // existence of the proof.
            //
            // The order at ACME will change status to either
            // confirm ownership of the domain, or fail due to the
            // not finding the proof. To see the change, we poll
            // the API with 5000 milliseconds wait between.
            challenge.validate(5000)?;

            log::info!("Validation for {} returned result. Updating state", self.domain);

            // Update the state against the ACME API.
            ord_new.refresh()?;
        };

        // Ownership is proven. Create a private key for
        // the certificate. These are provided for convenience, you
        // can provide your own keypair instead if you want.
        let pkey_pri = create_p384_key();

        log::info!(
            "Submitting completed request and awaiting certificate for {}",
            self.domain
        );

        // Submit the CSR. This causes the ACME provider to enter a
        // state of "processing" that must be polled until the
        // certificate is either issued or rejected. Again we poll
        // for the status change.
        let ord_cert = ord_csr.finalize_pkey(pkey_pri, 5000)?;

        // Now download the certificate. Also stores the cert in
        // the persistence.
        let cert = ord_cert.download_and_save_cert()?;

        log::info!("Retrieved certificate for {}", self.domain);

        self.load_cert(cert.certificate().as_bytes())?;

        // Set default values. Certificates and keys are stored externally
        self.private_key = String::new();
        self.certificate = String::new();
        self.cio_company_id = company.id;

        Ok(cert)
    }

    pub fn load_cert(&mut self, certificate: &[u8]) -> Result<()> {
        let expiration_date = Self::expiration_date(certificate)?;
        self.expiration_date = expiration_date.date_naive();

        let dur = expiration_date - Utc::now();
        self.valid_days_left = dur.num_days() as i32;

        log::info!(
            "Loaded cert metadata for {} ({} / {})",
            self.domain,
            self.expiration_date,
            self.valid_days_left
        );

        Ok(())
    }

    pub async fn load_from_reader<T>(&mut self, reader: &T) -> Result<()>
    where
        T: CertificateStorage,
    {
        self.load_cert(&reader.read_cert(&self.domain).await?)
    }

    /// Inspect the certificate to get the expiration_date.
    pub fn expiration_date(certificate: &[u8]) -> Result<DateTime<Utc>> {
        // load as x509
        let x509 = X509::from_pem(certificate)?;

        // convert asn1 time to Tm
        let not_after = format!("{}", x509.not_after());

        // Display trait produces this format, which is kinda dumb.
        // Apr 19 08:48:46 2019 GMT
        Ok(Utc.datetime_from_str(&not_after, "%h %e %H:%M:%S %Y %Z")?)
    }
}

/// Implement updating the Airtable record for a Certificate.
#[async_trait]
impl UpdateAirtableRecord<Certificate> for Certificate {
    async fn update_airtable_record(&mut self, _record: Certificate) -> Result<()> {
        Ok(())
    }
}

impl NewCertificate {
    pub async fn renew<'a>(
        &'a mut self,
        db: &'a Database,
        company: &'a Company,
        storage: &'a [Box<dyn SslCertificateStorage>],
    ) -> Result<()> {
        let renewed_certificate = self.create_cert(company).await?;

        log::info!("Renewed certificate for {}", self.domain);

        // Write the certificate and key to the requested locations
        for store in storage {
            store
                .write_cert(&self.domain, renewed_certificate.certificate().as_bytes())
                .await?;
            store
                .write_key(&self.domain, renewed_certificate.private_key().as_bytes())
                .await?;
        }

        log::info!("Stored certificate and key for {}", self.domain);

        // Update the database and Airtable.
        self.upsert(db).await?;

        Ok(())
    }
}

impl Certificate {
    pub async fn renew<'a>(
        &'a mut self,
        db: &'a Database,
        company: &'a Company,
        storage: &'a [Box<dyn SslCertificateStorage>],
    ) -> Result<()> {
        let mut cert: NewCertificate = self.clone().into();
        cert.renew(db, company, storage).await
    }
}

pub trait SslCertificateStorage: CertificateStorage + KeyStorage + Send + Sync + 'static {}
impl<T> SslCertificateStorage for T where T: CertificateStorage + KeyStorage + Send + Sync + 'static {}

#[async_trait]
pub trait CertificateStorage {
    async fn read_cert(&self, domain: &str) -> Result<Vec<u8>>;
    async fn write_cert(&self, domain: &str, data: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait KeyStorage {
    async fn write_key(&self, domain: &str, data: &[u8]) -> Result<()>;
}

pub struct GitHubBackend {
    client: octorust::Client,
    owner: String,
    repo: String,
}

impl GitHubBackend {
    pub fn new(client: octorust::Client, owner: String, repo: String) -> Self {
        Self { client, owner, repo }
    }

    async fn repo(&self) -> Result<FullRepository> {
        Ok(self.client.repos().get(&self.owner, &self.repo).await?.body)
    }

    fn path(&self, domain: &str, file: &str) -> String {
        format!("/nginx/ssl/{}/{}", domain.replace("*.", "wildcard."), file)
    }
}

#[async_trait]
impl CertificateStorage for GitHubBackend {
    async fn read_cert(&self, domain: &str) -> Result<Vec<u8>> {
        let (cert, _) = get_file_content_from_repo(
            &self.client,
            &self.owner,
            &self.repo,
            "", // if empty it uses the default branch
            &self.path(domain, "fullchain.pem"),
        )
        .await?;

        Ok(cert)
    }

    async fn write_cert(&self, domain: &str, data: &[u8]) -> Result<()> {
        let repo = self.repo().await?;
        create_or_update_file_in_github_repo(
            &self.client,
            &self.owner,
            &self.repo,
            &repo.default_branch,
            &self.path(domain, "fullchain.pem"),
            data.to_vec(),
        )
        .await?;

        Ok(())
    }
}

#[async_trait]
impl KeyStorage for GitHubBackend {
    async fn write_key(&self, domain: &str, data: &[u8]) -> Result<()> {
        let repo = self.repo().await?;

        create_or_update_file_in_github_repo(
            &self.client,
            &self.owner,
            &self.repo,
            &repo.default_branch,
            &self.path(domain, "privkey.pem"),
            data.to_vec(),
        )
        .await?;

        Ok(())
    }
}

pub struct GcsBackend<S> {
    client: Storage<S>,
    bucket: String,
    mime: Mime,
}

impl<S> GcsBackend<S>
where
    S: Send + Sync + Clone + google_storage1::hyper::service::Service<Uri> + 'static,
    S::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    S::Future: Send + Unpin + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    pub fn new(client: Storage<S>, bucket: String) -> Self {
        Self {
            client,
            bucket,
            mime: "application/x-pem-file".parse().unwrap(),
        }
    }

    fn path(&self, domain: &str, file_type: &str, file: &str) -> String {
        format!("ssl/{}/{}/{}", domain.replace("*.", "wildcard."), file_type, file)
    }
}

#[async_trait]
impl<S> CertificateStorage for GcsBackend<S>
where
    S: Send + Sync + Clone + google_storage1::hyper::service::Service<Uri> + 'static,
    S::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    S::Future: Send + Unpin + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    async fn read_cert(&self, domain: &str) -> Result<Vec<u8>> {
        let path = self.path(domain, "certificate", "fullchain.pem");
        let (response, _) = self.client.objects().get(&self.bucket, &path).doit().await?;
        let data = hyper::body::to_bytes(response.into_body()).await?;

        Ok(data.to_vec())
    }

    async fn write_cert(&self, domain: &str, data: &[u8]) -> Result<()> {
        let path = self.path(domain, "certificate", "fullchain.pem");
        let cursor = std::io::Cursor::new(data);

        let request = Object::default();
        self.client
            .objects()
            .insert(request, &self.bucket)
            .name(&path)
            .upload(cursor, self.mime.clone())
            .await?;

        Ok(())
    }
}

#[async_trait]
impl<S> KeyStorage for GcsBackend<S>
where
    S: Send + Sync + Clone + google_storage1::hyper::service::Service<Uri> + 'static,
    S::Response: Connection + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    S::Future: Send + Unpin + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    async fn write_key(&self, domain: &str, data: &[u8]) -> Result<()> {
        let path = self.path(domain, "key", "privkey.pem");
        let cursor = std::io::Cursor::new(data);

        let request = Object::default();
        self.client
            .objects()
            .insert(request, &self.bucket)
            .name(&path)
            .upload(cursor, self.mime.clone())
            .await?;

        Ok(())
    }
}
