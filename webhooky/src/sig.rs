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

    fn verify(body: &[u8], key: &[u8], signature: &[u8]) -> bool {
        if let Ok(mut mac) = <Self::Algo as Mac>::new_from_slice(key) {
            mac.update(body);
            mac.verify_slice(signature).is_ok()
        } else {
            false
        }
    }
}


#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    use super::SignatureVerification;

    #[test]
    fn test_verifies_new_signature() {
        struct Verifier;

        impl SignatureVerification for Verifier {
            type Algo = Hmac<Sha256>;
        }

        let test_key = "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=";
        let test_body = "{\"fake\": \"message\"}";

        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(test_key.as_bytes()).unwrap();
        mac.update(test_body.as_bytes());

        let result = mac.finalize().into_bytes();

        assert!(Verifier::verify(test_body.as_bytes(), test_key.as_bytes(), &result));
    }

    #[test]
    fn test_verifies_valid_github_signature() {
        struct Verifier;

        impl SignatureVerification for Verifier {
            type Algo = Hmac<Sha256>;
        }

        let test_key = "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=";
        let test_signature = hex::decode("318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19").unwrap();
        let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

        assert!(Verifier::verify(test_body.as_bytes(), test_key.as_bytes(), &test_signature));
    }

    #[test]
    fn test_verifies_invalid_signature() {
        struct Verifier;

        impl SignatureVerification for Verifier {
            type Algo = Hmac<Sha256>;
        }

        let test_key = "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=";
        let test_signature = hex::decode("318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada18").unwrap();
        let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

        assert!(!Verifier::verify(test_body.as_bytes(), test_key.as_bytes(), &test_signature));
    }
}
