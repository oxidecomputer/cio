use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    exp: usize,
}

pub fn token(key: String, secret: String) -> String {
    let claims = Claims {
        iss: key,
        exp: 10000000000, // TODO: make this a value in seconds.
    };

    let mut header = Header::default();
    header.kid = Some("signing_key".to_owned());
    header.alg = Algorithm::HS256;

    let token = match encode(
        &header,
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    ) {
        Ok(t) => t,
        Err(e) => panic!("creating jwt failed: {}", e), // TODO: return the error.
    };

    return token;
}
