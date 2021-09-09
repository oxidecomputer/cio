use std::{env, fmt, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use diesel::{pg::PgConnection, r2d2};

#[derive(Debug, Clone)]
pub struct Database {
    pool: DB,
}

#[derive(Clone)]
struct DB(Arc<r2d2::Pool<r2d2::ConnectionManager<PgConnection>>>);

impl Default for Database {
    fn default() -> Self {
        let database_url = env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        let manager = r2d2::ConnectionManager::new(&database_url);
        let pool = r2d2::Pool::builder().max_size(15).build(manager).unwrap();

        Database {
            pool: DB(Arc::new(pool)),
        }
    }
}

// This is a workaround so we can implement Debug for PgConnection.
impl fmt::Debug for DB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DB").finish()
    }
}

impl Database {
    /// Establish a connection to the database.
    pub fn new() -> Database {
        Default::default()
    }

    /// Returns a connection from the pool.
    pub fn conn(&self) -> r2d2::PooledConnection<r2d2::ConnectionManager<PgConnection>> {
        self.pool
            .0
            .get()
            .unwrap_or_else(|e| panic!("getting a connection from the pool failed: {}", e))
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
