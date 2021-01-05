use std::env;
use std::sync::Arc;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2;
use tracing::instrument;

use crate::models::{NewRFD, RFD};
use crate::schema::rfds;

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
        self.pool.get().unwrap_or_else(|e| panic!("getting a connection from the pool failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_rfds(&self) -> Vec<RFD> {
        rfds::dsl::rfds.order_by(rfds::dsl::id.desc()).load::<RFD>(&self.conn()).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_rfd(&self, number: i32) -> Option<RFD> {
        match rfds::dsl::rfds.filter(rfds::dsl::number.eq(number)).limit(1).load::<RFD>(&self.conn()) {
            Ok(r) => {
                if !r.is_empty() {
                    return Some(r.get(0).unwrap().clone());
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the rfd with number {} in the database", number, e);
                return None;
            }
        }

        None
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_rfd(&self, rfd: &NewRFD) -> RFD {
        // See if we already have the rfd in the database.
        if let Some(r) = self.get_rfd(rfd.number) {
            // Update the rfd.
            return diesel::update(&r)
                .set(rfd)
                .get_result::<RFD>(&self.conn())
                .unwrap_or_else(|e| panic!("unable to update rfd {}: {}", r.id, e));
        }

        diesel::insert_into(rfds::table)
            .values(rfd)
            .get_result(&self.conn())
            .unwrap_or_else(|e| panic!("creating rfd failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn update_rfd(&self, rfd: &RFD) -> RFD {
        // Update the rfd.
        diesel::update(rfd)
            .set(rfd.clone())
            .get_result::<RFD>(&self.conn())
            .unwrap_or_else(|e| panic!("unable to update rfd {}: {}", rfd.id, e))
    }
}
