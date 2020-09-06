use std::env;

use diesel::pg::PgConnection;
use diesel::prelude::*;

/*use crate::models::Applicant;
use crate::schema::applicants;*/
use crate::code_that_should_be_generated::{Building, GithubLabel, Link, User};
use crate::configs::{BuildingConfig, LabelConfig, LinkConfig, UserConfig};
use crate::schema::{buildings, github_labels, links, users};

pub struct Database {
    conn: PgConnection,
}

impl Default for Database {
    fn default() -> Self {
        let database_url =
            env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        Database {
            conn: PgConnection::establish(&database_url).unwrap_or_else(|e| {
                panic!("error connecting to {}: {}", database_url, e)
            }),
        }
    }
}

// TODO: more gracefully handle errors
// TODO: possibly generate all this boilerplate as well.
// TODO: generate all the diesal duplicates as well.
impl Database {
    /// Establish a connection to the database.
    pub fn new() -> Database {
        Default::default()
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
                let a = r.get(0).unwrap();


                // Update the applicant.
                return diesel::update(a)
                    .set(applicant)
                    .get_result::<Applicant>(&self.conn)
                    .expect(&format!("unable to update applicant {}", a.id));
            }
        }*/

        diesel::insert_into(applicants::table)
            .values(applicant)
            .get_result(&self.conn)
            .expect("creating applicant failed")
    }*/

    pub fn upsert_building(&self, building: &BuildingConfig) -> Building {
        // See if we already have the building in the database.
        match buildings::dsl::buildings
            .filter(buildings::dsl::name.eq(building.name.to_string()))
            .limit(1)
            .load::<Building>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the building in the database so we need to add it.
                    // That will happen below.
                } else {
                    let b = r.get(0).unwrap();

                    // Update the building.
                    return diesel::update(b)
                        .set(building)
                        .get_result::<Building>(&self.conn)
                        .unwrap_or_else(|e| {
                            panic!("unable to update building {}: {}", b.id, e)
                        });
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the building in the database, adding it", e);
            }
        }

        diesel::insert_into(buildings::table)
            .values(building)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating building failed: {}", e))
    }

    pub fn upsert_github_label(
        &self,
        github_label: &LabelConfig,
    ) -> GithubLabel {
        // See if we already have the github_label in the database.
        match github_labels::dsl::github_labels
            .filter(github_labels::dsl::name.eq(github_label.name.to_string()))
            .limit(1)
            .load::<GithubLabel>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the github_label in the database so we need to add it.
                    // That will happen below.
                } else {
                    let label = r.get(0).unwrap();

                    // Update the github_label.
                    return diesel::update(label)
                        .set(github_label)
                        .get_result::<GithubLabel>(&self.conn)
                        .unwrap_or_else(|e| {
                            panic!(
                                "unable to update github_label {}: {}",
                                label.id, e
                            )
                        });
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the github_label in the database, adding it", e);
            }
        }

        diesel::insert_into(github_labels::table)
            .values(github_label)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating github_label failed: {}", e))
    }

    pub fn upsert_link(&self, link: &LinkConfig) -> Link {
        // See if we already have the link in the database.
        match links::dsl::links
            .filter(links::dsl::name.eq(link.name.to_string()))
            .limit(1)
            .load::<Link>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the link in the database so we need to add it.
                    // That will happen below.
                } else {
                    let l = r.get(0).unwrap();

                    // Update the link.
                    return diesel::update(l)
                        .set(link)
                        .get_result::<Link>(&self.conn)
                        .unwrap_or_else(|e| {
                            panic!("unable to update link {}: {}", l.id, e)
                        });
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the link in the database, adding it", e);
            }
        }

        diesel::insert_into(links::table)
            .values(link)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating link failed: {}", e))
    }

    pub fn upsert_user(&self, user: &UserConfig) -> User {
        // See if we already have the user in the database.
        match users::dsl::users
            .filter(users::dsl::username.eq(user.username.to_string()))
            .limit(1)
            .load::<User>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the user in the database so we need to add it.
                    // That will happen below.
                } else {
                    let u = r.get(0).unwrap();

                    // Update the user.
                    return diesel::update(u)
                        .set(user)
                        .get_result::<User>(&self.conn)
                        .unwrap_or_else(|e| {
                            panic!("unable to update user {}: {}", u.id, e)
                        });
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the user in the database, adding it", e);
            }
        }

        diesel::insert_into(users::table)
            .values(user)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating user failed: {}", e))
    }
}
