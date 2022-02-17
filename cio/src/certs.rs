#![allow(clippy::from_over_into)]
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::from_utf8,
    time,
};

use acme_lib::{create_p384_key, persist::FilePersist, Directory, DirectoryUrl};
use anyhow::Result;
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_humanize::HumanTime;
use cloudflare::endpoints::dns;
use log::info;
use macros::db;
use openssl::x509::X509;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockText, MessageBlockType, MessageType,
};

use crate::{
    airtable::AIRTABLE_CERTIFICATES_TABLE,
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    dns_providers::DNSProviderOps,
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
}

/// Convert the certificate into a Slack message.
impl From<NewCertificate> for FormattedMessage {
    fn from(item: NewCertificate) -> Self {
        let dur = item.expiration_date - Utc::now().date().naive_utc();
        let human_date = HumanTime::from(dur);

        let mut text = format!("`{}` certificate renewed", item.domain);
        if !item.repos.is_empty() {
            text += &format!(
                "\nupdated `{}`, `{}` secrets in the following repos: `{}`",
                item.certificate_github_actions_secret_name,
                item.private_key_github_actions_secret_name,
                item.repos.join("`, `")
            );
        }

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: crate::colors::Colors::Green.to_string(),
                author_icon: Default::default(),
                author_link: Default::default(),
                author_name: Default::default(),
                fallback: Default::default(),
                fields: Default::default(),
                footer: Default::default(),
                footer_icon: Default::default(),
                image_url: Default::default(),
                pretext: Default::default(),
                text: Default::default(),
                thumb_url: Default::default(),
                title: Default::default(),
                title_link: Default::default(),
                ts: Default::default(),
                blocks: vec![
                    MessageBlock {
                        block_type: MessageBlockType::Section,
                        text: Some(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text,
                        }),
                        elements: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: format!("SSL cert | _expires {}_", human_date),
                        })],
                        text: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                ],
            }],
        }
    }
}

impl From<Certificate> for FormattedMessage {
    fn from(item: Certificate) -> Self {
        let new: NewCertificate = item.into();
        new.into()
    }
}

impl NewCertificate {
    // Send a slack notification to the channels in the object.
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let mut msg: FormattedMessage = self.clone().into();

        for channel in &self.notify_slack_channels {
            // Set the channel.
            msg.channel = channel.to_string();
            // Post the message.
            company.post_to_slack_channel(db, &msg).await?;
        }

        Ok(())
    }

    /// Creates a Let's Encrypt SSL certificate for a domain by using a DNS challenge.
    /// The DNS Challenge TXT record is added to Cloudflare automatically.
    pub async fn create_cert(&mut self, company: &Company) -> Result<()> {
        let api_client = company.authenticate_cloudflare()?;

        // Save/load keys and certificates to a temporary directory, we will re-save elsewhere.
        let persist = FilePersist::new(env::temp_dir());

        // Create a directory entrypoint.
        // Use DirectoryUrl::LetsEncrypStaging for dev/testing.
        let dir = Directory::from_url(persist, DirectoryUrl::LetsEncrypt)?;

        // Reads the private account key from persistence, or
        // creates a new one before accessing the API to establish
        // that it's there.
        let acc = dir.account(&company.gsuite_subject)?;

        // Order a new TLS certificate for a domain.
        let mut ord_new = acc.new_order(&self.domain, &[])?;

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
            let auths = ord_new.authorizations()?;

            // Get the proff we need for the TXT record:
            // _acme-challenge.<domain-to-be-proven>.  TXT  <proof>
            let challenge = auths[0].dns_challenge();

            // Create a TXT record for _acme-challenge.{domain} with the value of
            // the proof.
            // Use the Cloudflare API for this.
            let record_name = format!("_acme-challenge.{}", &self.domain.replace("*.", ""));

            let content = dns::DnsContent::TXT {
                content: challenge.dns_proof(),
            };

            // Ensure our DNS record exists.
            api_client.ensure_record(&record_name, content).await?;

            // TODO: make this less awful than a sleep.
            info!("validating the proof...");
            let dur = time::Duration::from_secs(10);
            tokio::time::sleep(dur).await;

            // After the TXT record is accessible, the calls
            // this to tell the ACME API to start checking the
            // existence of the proof.
            //
            // The order at ACME will change status to either
            // confirm ownership of the domain, or fail due to the
            // not finding the proof. To see the change, we poll
            // the API with 5000 milliseconds wait between.
            challenge.validate(5000)?;

            // Update the state against the ACME API.
            ord_new.refresh()?;
        };

        // Ownership is proven. Create a private key for
        // the certificate. These are provided for convenience, you
        // can provide your own keypair instead if you want.
        let pkey_pri = create_p384_key();

        // Submit the CSR. This causes the ACME provider to enter a
        // state of "processing" that must be polled until the
        // certificate is either issued or rejected. Again we poll
        // for the status change.
        let ord_cert = ord_csr.finalize_pkey(pkey_pri, 5000)?;

        // Now download the certificate. Also stores the cert in
        // the persistence.
        let cert = ord_cert.download_and_save_cert()?;

        self.private_key = cert.private_key().to_string();
        self.certificate = cert.certificate().to_string();
        self.valid_days_left = cert.valid_days_left() as i32;
        self.expiration_date = crate::utils::default_date();
        self.cio_company_id = company.id;

        Ok(())
    }

    /// For a certificate struct, populate the certificate fields for the domain.
    /// This will create the cert from Let's Encrypt and update Cloudflare TXT records for the
    /// verification.
    pub async fn populate(&mut self, company: &Company) -> Result<()> {
        self.create_cert(company).await?;

        let exp_date = self.expiration_date();
        self.expiration_date = exp_date.date().naive_utc();
        self.valid_days_left = self.valid_days_left();

        Ok(())
    }

    /// For a certificate struct, populate the certificate and private_key fields from
    /// GitHub, then fill in the rest.
    pub async fn populate_from_github(&mut self, github: &octorust::Client, company: &Company) -> Result<()> {
        let owner = &company.github_org;
        let repo = "configs";

        if let Ok((cert, _)) = get_file_content_from_repo(
            github,
            owner,
            repo,
            "", // if empty it uses the default branch
            &self.get_github_path("fullchain.pem"),
        )
        .await
        {
            if !cert.is_empty() {
                self.certificate = from_utf8(&cert)?.to_string();
            }
        }

        if let Ok((p, _)) = get_file_content_from_repo(
            github,
            owner,
            repo,
            "", // if empty it uses the default branch
            &self.get_github_path("privkey.pem"),
        )
        .await
        {
            if !p.is_empty() {
                self.private_key = from_utf8(&p)?.to_string();
            }
        }

        let exp_date = self.expiration_date();
        self.expiration_date = exp_date.date().naive_utc();
        self.valid_days_left = self.valid_days_left();

        Ok(())
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
        format!("/nginx/ssl/{}/{}", self.domain.replace("*.", "wildcard."), file)
    }

    /// Saves the fullchain certificate and privkey to /{dir}/{domain}/{privkey.pem,fullchain.pem}
    pub fn save_to_directory(&self, dir: &str) -> Result<()> {
        if self.certificate.is_empty() {
            // Return early.
            return Ok(());
        }

        let path = self.get_path(dir);

        // Create the directory if it does not exist.
        fs::create_dir_all(path.clone())?;

        // Write the files.
        fs::write(path.join("fullchain.pem"), self.certificate.as_bytes())?;
        fs::write(path.join("privkey.pem"), self.private_key.as_bytes())?;

        Ok(())
    }

    /// Saves the fullchain certificate and privkey to the configs github repo.
    pub async fn save_to_github_repo(&self, github: &octorust::Client, company: &Company) -> Result<()> {
        if self.certificate.is_empty() {
            // Return early.
            return Ok(());
        }

        let owner = &company.github_org;
        let repo = "configs";
        let r = github.repos().get(owner, repo).await?;

        // Write the files.
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("fullchain.pem"),
            self.certificate.as_bytes().to_vec(),
        )
        .await?;
        create_or_update_file_in_github_repo(
            github,
            owner,
            repo,
            &r.default_branch,
            &self.get_github_path("privkey.pem"),
            self.private_key.as_bytes().to_vec(),
        )
        .await?;

        Ok(())
    }

    /// For the repos given, update the GitHub actions secrets with the new cert and key.
    pub async fn update_github_action_secrets(&self, github: &octorust::Client, company: &Company) -> Result<()> {
        if self.repos.is_empty()
            || self.certificate_github_actions_secret_name.is_empty()
            || self.private_key_github_actions_secret_name.is_empty()
        {
            // If we have no repos to update return early.
            return Ok(());
        }

        let mut plain_text: BTreeMap<String, String> = Default::default();
        plain_text.insert(
            self.certificate_github_actions_secret_name.to_string(),
            self.certificate.to_string(),
        );
        plain_text.insert(
            self.private_key_github_actions_secret_name.to_string(),
            self.private_key.to_string(),
        );

        for repo in &self.repos {
            // First let's encrypt the secrets for the repo.
            // This uses the repo's public key.
            let (key_id, secrets) = crate::utils::encrypt_github_secrets(github, company, repo, &plain_text).await?;

            // Update each secret.
            for (name, secret) in secrets {
                github
                    .actions()
                    .create_or_update_repo_secret(
                        &company.github_org,
                        repo,
                        &name,
                        &octorust::types::ActionsCreateUpdateRepoSecretRequest {
                            encrypted_value: secret.to_string(),
                            key_id: key_id.to_string(),
                        },
                    )
                    .await?;
            }
        }

        Ok(())
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
    async fn update_airtable_record(&mut self, _record: Certificate) -> Result<()> {
        Ok(())
    }
}

impl Certificate {
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let n: NewCertificate = self.into();
        n.send_slack_notification(db, company).await
    }

    pub async fn renew(&self, db: &Database, github: &octorust::Client, company: &Company) -> Result<()> {
        let mut cert: NewCertificate = self.into();

        cert.populate(company).await?;

        // Save the certificate to disk.
        cert.save_to_github_repo(github, company).await?;

        // Update the Github Action secrets, with the new certificates if there are some.
        cert.update_github_action_secrets(github, company).await?;

        // Update the database and Airtable.
        cert.upsert(db).await?;

        // Send the notification we renewed the cert.
        cert.send_slack_notification(db, company).await?;

        Ok(())
    }
}
