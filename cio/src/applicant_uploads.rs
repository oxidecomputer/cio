use async_bb8_diesel::{AsyncConnection, AsyncRunQueryDsl, PoolError};
use chrono::{DateTime, Duration, Utc};
use diesel::{
    result::Error as DieselError, AsChangeset, ExpressionMethods, Insertable, QueryDsl, Queryable, RunQueryDsl,
};
use ring::rand::{SecureRandom, SystemRandom};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, ops::DerefMut};

use crate::{db::Database, schema::upload_tokens};

#[derive(Debug, Queryable, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = upload_tokens)]
pub struct UploadToken {
    pub id: i32,
    pub email: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct UploadTokenStore {
    db: Database,
    token_lifetime: Duration,
}

impl UploadTokenStore {
    pub fn new(db: Database, lifetime: Duration) -> Self {
        Self {
            db,
            token_lifetime: lifetime,
        }
    }

    pub async fn get(&self, email: &str) -> Result<UploadToken, UploadTokenError> {
        let pool = self.db.pool();
        let email = email.to_string();
        let token_lifetime = self.token_lifetime;

        pool.transaction(move |conn| {
            let token = upload_tokens::dsl::upload_tokens
                .filter(upload_tokens::dsl::email.eq(email.clone()))
                .filter(upload_tokens::dsl::expires_at.gt(Utc::now()))
                .filter(upload_tokens::dsl::used_at.is_null())
                .first::<UploadToken>(conn.deref_mut());

            token.or_else(|lookup_err| match lookup_err {
                DieselError::NotFound => {
                    log::info!("No valid upload tokens found for email. Creating new token");

                    UploadTokenStore::generate_key().and_then(|token| {
                        diesel::insert_into(upload_tokens::table)
                            .values((
                                upload_tokens::dsl::email.eq(email),
                                upload_tokens::dsl::token.eq(hex::encode(token)),
                                upload_tokens::dsl::expires_at.eq(Utc::now() + token_lifetime),
                            ))
                            .get_result(conn.deref_mut())
                            .map_err(UploadTokenError::DB)
                    })
                }
                err => Err(UploadTokenError::DB(err)),
            })
        })
        .await
    }

    pub async fn test(&self, email: String, token: String) -> Result<bool, UploadTokenError> {
        Ok(upload_tokens::dsl::upload_tokens
            .filter(upload_tokens::dsl::email.eq(email))
            .filter(upload_tokens::dsl::token.eq(token))
            .filter(upload_tokens::dsl::expires_at.lt(Utc::now()))
            .filter(upload_tokens::dsl::used_at.is_null())
            .first_async::<UploadToken>(self.db.pool())
            .await
            .map(|_| true)?)
    }

    pub async fn consume(&self, email: &str, token: &str) -> Result<UploadToken, UploadTokenError> {
        let target = upload_tokens::dsl::upload_tokens
            .filter(upload_tokens::dsl::email.eq(email.to_string()))
            .filter(upload_tokens::dsl::token.eq(token.to_string()))
            .filter(upload_tokens::dsl::expires_at.lt(Utc::now()))
            .filter(upload_tokens::dsl::used_at.is_null());

        let token = diesel::update(target)
            .set(upload_tokens::dsl::used_at.eq(Utc::now()))
            .get_result_async::<UploadToken>(self.db.pool())
            .await?;

        Ok(token)
    }

    fn generate_key() -> Result<[u8; 32], UploadTokenError> {
        let rng = SystemRandom::new();
        let mut key = [0; 32];
        rng.fill(&mut key).map_err(|_| UploadTokenError::FailedToGenerate)?;

        Ok(key)
    }
}

#[derive(Debug)]
pub enum UploadTokenError {
    AsyncDB(PoolError),
    DB(DieselError),
    FailedToGenerate,
}

impl From<PoolError> for UploadTokenError {
    fn from(error: PoolError) -> Self {
        UploadTokenError::AsyncDB(error)
    }
}

impl From<DieselError> for UploadTokenError {
    fn from(error: DieselError) -> Self {
        UploadTokenError::DB(error)
    }
}

impl fmt::Display for UploadTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UploadTokenError::AsyncDB(err) => write!(f, "Upload token database interaction failed due to {}", err),
            UploadTokenError::DB(err) => write!(f, "Upload token database interaction failed due to {}", err),
            UploadTokenError::FailedToGenerate => write!(f, "Failed to generate a random token"),
        }
    }
}

impl Error for UploadTokenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            UploadTokenError::AsyncDB(err) => Some(err),
            UploadTokenError::DB(err) => Some(err),
            UploadTokenError::FailedToGenerate => None,
        }
    }
}
