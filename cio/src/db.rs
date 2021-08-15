use std::{env, sync::Arc};

use diesel::{pg::PgConnection, r2d2};

pub struct Database {
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<PgConnection>>>,
}

impl Default for Database {
    fn default() -> Self {
        let database_url = env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        let manager = r2d2::ConnectionManager::new(&database_url);
        let pool = r2d2::Pool::builder().max_size(15).build(manager).unwrap();

        Database { pool: Arc::new(pool) }
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
            .get()
            .unwrap_or_else(|e| panic!("getting a connection from the pool failed: {}", e))
    }
}
