use async_trait::async_trait;
use dropshot::{RequestContext, ExtractorMetadata, UntypedBody, HttpError, ServerContext, Extractor};
use hmac::Hmac;
use sha2::Sha256;
use std::sync::Arc;

use crate::{sig::HmacSignatureVerifier, http::{forbidden, unauthorized, Headers}};

pub struct CheckrVerification;

#[async_trait]
impl HmacSignatureVerifier for CheckrVerification {
    type Algo = Hmac<Sha256>;

    async fn key<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
        Ok(std::env::var("GITHUB_KEY").map(|key| Cow::Owned(key.into_bytes()))?)
    }

    async fn signature<'a, Context: ServerContext>(rqctx: &'a Arc<RequestContext<Context>>) -> Result<Cow<'a, [u8]>> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let signature = headers.0.get("X-Checkr-Signature").and_then(|header_value| {
            header_value.to_str().ok()
        }).and_then(|header| {
            hex::decode(header.trim_start_matches("sha256")).ok()
        }).ok_or_else(unauthorized)?;

        Ok(Cow::Owned(signature))
    }
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    use crate::sig::SignatureVerification;

    #[test]
    fn test_verifies_valid_github_signature() {
        let test_key = "vkPkH4G2k8XNC5HWA6QgZd08v37P8KcVZMjaP4zgGWc=";
        let test_signature = hex::decode("318376db08607eb984726533b1d53430e31c4825fd0d9b14e8ed38e2a88ada19").unwrap();
        let test_body = include_str!("../tests/github_webhook_sig_test.json").trim();

        assert!(CheckrVerification::new(test_key.as_bytes(), &test_signature, test_body.as_bytes()).is_ok());
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

        assert!(CheckrVerification::new(test_key.as_bytes(), &test_signature, test_body.as_bytes()).is_err());
    }
}
