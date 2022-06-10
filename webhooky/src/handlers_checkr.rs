use async_trait::async_trait;
use dropshot::{RequestContext, ExtractorMetadata, UntypedBody, HttpError, ServerContext, Extractor};
use hmac::Hmac;
use sha2::Sha256;
use std::sync::Arc;

use crate::{sig::SignatureVerification, http::{forbidden, unauthorized, Headers}};

pub struct CheckrVerification;

impl SignatureVerification for CheckrVerification {
    type Algo = Hmac<Sha256>;
}

#[async_trait]
impl Extractor for CheckrVerification {
    async fn from_request<Context: ServerContext>(rqctx: Arc<RequestContext<Context>>) -> Result<CheckrVerification, HttpError> {
        let headers = Headers::from_request(rqctx.clone()).await?;
        let body = UntypedBody::from_request(rqctx.clone()).await?;
        let signature = headers.0.get("X-Checkr-Signature").and_then(|header_value| {
            header_value.to_str().ok()
        }).and_then(|header| {
            hex::decode(header.trim_start_matches("sha256")).ok()
        }).ok_or_else(unauthorized)?;
        
        // TODO: Lookup Oxide company to get API key

        let verified = CheckrVerification::verify(body.as_bytes(), "key".as_bytes(), &signature);

        if verified {
            Ok(CheckrVerification)
        } else {
            Err(forbidden())
        }
    }

    fn metadata() -> ExtractorMetadata {
        ExtractorMetadata {
            paginated: false,
            parameters: vec![],
        }
    }
}