#![allow(clippy::from_over_into)]
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use google_storage1::{
    api::{Object, Storage},
    hyper,
    hyper::client::connect::Connection,
    hyper::Uri,
};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, OrderStatus,
};
use macros::db;
use mime::Mime;
use octorust::types::FullRepository;
use openssl::x509::X509;
use rcgen::{Certificate as GeneratedCertificate, CertificateParams, DistinguishedName};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env::var;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::sleep,
};

use crate::{
    airtable::AIRTABLE_CERTIFICATES_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    dns_providers::{DNSProviderOps, DnsRecord, DnsRecordType, DnsUpdateMode},
    schema::certificates,
    utils::{create_or_update_file_in_github_repo, get_file_content_from_repo, SliceExt},
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

pub struct AcmeCertificate {
    private_key: Vec<u8>,
    certificate_chain: Vec<u8>,
}

impl NewCertificate {
    /// Creates a Let's Encrypt SSL certificate for a domain by using a DNS challenge.
    /// The DNS Challenge TXT record is added to Cloudflare automatically.
    pub async fn create_cert(&mut self, company: &Company) -> Result<AcmeCertificate> {
        let api_client = company.authenticate_dns_providers().await?;

        let account = Account::create(
            &NewAccount {
                contact: &[&var("CERT_ACCOUNT")?],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            LetsEncrypt::Production.url(),
            None,
        )
        .await?;

        log::info!("Authenticated with cert provider");

        let mut domains = vec![self.domain.clone()];
        domains.extend(self.sans.clone());

        let identifiers = domains.clone().into_iter().map(Identifier::Dns).collect::<Vec<_>>();
        let mut order = account
            .new_order(&NewOrder {
                identifiers: &identifiers,
            })
            .await?;

        let state = order.state();

        log::info!("Created cert order state: {:?}", state);

        let authorizations = order.authorizations().await?;
        let mut challenges = Vec::with_capacity(authorizations.len());

        log::info!("Retrieved authorization credentials");

        for authz in &authorizations {
            log::info!("Handling authorization for {:?}", authz.identifier);

            match &authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                unhandled => return Err(anyhow!("Unhandled cert authorization status: {:?}", unhandled)),
            }

            // We'll use the DNS challenges for this example, but you could
            // pick something else to use here.

            let challenge = authz
                .challenges
                .iter()
                .find(|c| c.r#type == ChallengeType::Dns01)
                .ok_or_else(|| anyhow::anyhow!("Failed to find cert DNS challenge: {:?}", authz.challenges))?;

            let Identifier::Dns(identifier) = &authz.identifier;

            // Create a TXT record for _acme-challenge.{domain} with the value of the proof.
            let record_name = format!("_acme-challenge.{}", identifier);

            // Ensure our DNS record exists.
            api_client
                .ensure_record(
                    DnsRecord {
                        name: record_name.to_string(),
                        type_: DnsRecordType::TXT,
                        content: order.key_authorization(challenge).dns_value(),
                    },
                    DnsUpdateMode::Replace,
                )
                .await?;

            challenges.push((identifier, &challenge.url));
        }

        for (_, url) in &challenges {
            order.set_challenge_ready(url).await?;
        }

        let mut delay = Duration::from_millis(5000);

        for i in 0..5 {
            sleep(delay).await;
            let state = order.refresh().await?;
            if let OrderStatus::Ready = state.status {
                log::info!("Reached final order state: {state:?}");
                break;
            } else {
                log::info!(
                    "Order for {:?} is not yet in a final state. It is currently in {state:?}",
                    domains
                );
            }

            delay *= 2;

            if i < 5 {
                log::info!("Order is not ready on attempt {i}, waiting {delay:?}");
            } else {
                return Err(anyhow::anyhow!("Order ready checks ran out of attempts"));
            }
        }

        let state = order.state();

        if state.status != OrderStatus::Ready {
            return Err(anyhow::anyhow!("UNhandled order status: {:?}", state.status));
        }

        log::info!("Order is ready, creating CSR for {:?}", domains);

        let mut params = CertificateParams::new(domains.clone());
        params.distinguished_name = DistinguishedName::new();
        let cert = GeneratedCertificate::from_params(params)?;
        let csr = cert.serialize_request_der()?;

        log::info!("Finalizing CSR for {:?}", domains);

        order.finalize(&csr).await?;

        let mut attempt = 1;
        let cert_chain_pem = loop {
            match order.certificate().await? {
                Some(cert_chain_pem) => break cert_chain_pem,
                None => {
                    sleep(Duration::from_secs(1)).await;
                    attempt += 1;

                    if attempt > 10 {
                        return Err(anyhow!("Exhausted attempts to retrieve certificate"));
                    }
                }
            }
        };

        let certificate = AcmeCertificate {
            private_key: cert.serialize_private_key_pem().as_bytes().to_vec(),
            certificate_chain: cert_chain_pem.as_bytes().to_vec(),
        };

        log::info!("Retrieved certificate for {:?}", domains);

        self.load_cert(&certificate.certificate_chain)?;

        // Set default values. Certificates and keys are stored externally
        self.private_key = String::new();
        self.certificate = String::new();
        self.cio_company_id = company.id;

        Ok(certificate)
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
                .write_cert(&self.domain, &renewed_certificate.certificate_chain)
                .await?;
            store.write_key(&self.domain, &renewed_certificate.private_key).await?;
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
            data.to_vec().trim(),
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
            data.to_vec().trim(),
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
