use std::env;

#[cfg(test)]
use diesel::debug_query;

use diesel::pg::PgConnection;
use diesel::prelude::*;

/*use crate::models::Applicant;
use crate::schema::applicants;*/

pub struct Database {
    conn: PgConnection,
}

impl Database {
    /// Establish a connection to the database.
    pub fn new() -> Database {
        let database_url =
            env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        Database {
            conn: PgConnection::establish(&database_url)
                .expect(&format!("error connecting to {}", database_url)),
        }
    }

    /* pub fn upsert_applicant(&self, applicant: &Applicant) -> Applicant {
        /*use crate::schema::applicants::dsl::*;
        // See if we already have the applicant in the database.
        let results = applicants
            .filter(email.eq(applicant.email), sheet_id.eq(applicant.sheet_id))
            .limit(1)
            .load::<Applicant>(&self.conn);

        if results.is_err() {
            println!("[db] on err: we don't have the applicant in the database, adding them")
        }

        if results.is_ok() {
            let r = results.unwrap();
            if r.is_empty() {
                println!(
                    "[db] on empty: we don't have the applicant in the database, adding them"
                )
            } else {
                let id = r.get(0).unwrap().id;

                applicant.id = id;

                // Update the applicant.
                return diesel::update(applicants.find(id))
                    .set(&applicant)
                    .get_result::<Applicant>(&self.conn)
                    .expect(&format!("unable to find applicant {}", id));
            }
        }*/

        diesel::insert_into(applicants::table)
            .values(&applicant)
            .get_result(&self.conn)
            .expect("creating applicant failed")
    }*/
}
