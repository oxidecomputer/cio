use anyhow::Result;
use digest::{KeyInit};
use hmac::{Mac};

// listen_checkr_background_update_webhooks
// listen_docusign_envelope_update_webhooks
// listen_emails_incoming_sendgrid_parse_webhooks
// listen_github_webhooks
// listen_mailchimp_mailing_list_webhooks
// listen_mailchimp_rack_line_webhooks
// listen_shippo_tracking_update_webhooks
// listen_slack_commands_webhooks

pub trait SignatureVerification {
    type Algo: Mac + KeyInit;

    fn verify(body: &[u8], key: &[u8], signature: &[u8]) -> Result<bool> {
        let mut mac = <Self::Algo as Mac>::new_from_slice(key)?;
        mac.update(body);

        Ok(mac.verify_slice(signature).map(|_| true)?)
    }
}