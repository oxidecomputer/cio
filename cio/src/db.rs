use std::{env, fmt};

use anyhow::Result;
use async_bb8_diesel::ConnectionManager;
use async_trait::async_trait;
use diesel::PgConnection;
use diesel_sentry::SentryConnection;

pub type DbConnection = SentryConnection<PgConnection>;
//pub type DbConnection = PgConnection;

#[derive(Debug, Clone)]
pub struct Database {
    pool: DB,
}

#[derive(Clone)]
struct DB(bb8::Pool<ConnectionManager<DbConnection>>);

// This is a workaround so we can implement Debug for PgConnection.
impl fmt::Debug for DB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DB").finish()
    }
}

impl Database {
    /// Establish a connection to the database.
    pub async fn new() -> Self {
        let database_url = env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        let manager = ConnectionManager::<DbConnection>::new(&database_url);
        let pool = bb8::Builder::new().build_unchecked(manager);

        Database { pool: DB(pool) }
    }

    /// Returns a reference to the underlying pool.
    pub fn pool(&self) -> &bb8::Pool<ConnectionManager<DbConnection>> {
        &self.pool.0
    }
}

#[async_trait]
impl steno::SecStore for Database {
    async fn saga_create(&self, create_params: steno::SagaCreateParams) -> Result<()> {
        crate::functions::Function::from_saga_create_params(self, &create_params).await?;

        Ok(())
    }

    async fn record_event(&self, event: steno::SagaNodeEvent) {
        crate::functions::Function::from_saga_node_event(self, &event)
            .await
            .unwrap();
    }

    async fn saga_update(&self, id: steno::SagaId, update: steno::SagaCachedState) {
        crate::functions::Function::from_saga_cached_state(self, &id, &update)
            .await
            .unwrap();
    }
}
