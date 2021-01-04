use std::env;
use std::sync::Arc;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2;
use tracing::instrument;

use crate::journal_clubs::{JournalClubMeeting, JournalClubPaper, NewJournalClubMeeting, NewJournalClubPaper};
use crate::models::{GithubRepo, MailingListSubscriber, NewMailingListSubscriber, NewRFD, NewRepo, RFD};
use crate::schema::{github_repos, journal_club_meetings, journal_club_papers, mailing_list_subscribers, rfds};

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
    pub fn get_github_repos(&self) -> Vec<GithubRepo> {
        github_repos::dsl::github_repos.order_by(github_repos::dsl::id.desc()).load::<GithubRepo>(&self.conn()).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_github_repo(&self, github_repo: &NewRepo) -> GithubRepo {
        // See if we already have the github_repo in the database.
        match github_repos::dsl::github_repos
            .filter(github_repos::dsl::full_name.eq(github_repo.full_name.to_string()))
            .limit(1)
            .load::<GithubRepo>(&self.conn())
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the github_repo in the database so we need to add it.
                    // That will happen below.
                } else {
                    let a = r.get(0).unwrap();

                    // Update the github_repo.
                    return diesel::update(a)
                        .set(github_repo)
                        .get_result::<GithubRepo>(&self.conn())
                        .unwrap_or_else(|e| panic!("unable to update github_repo {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the github_repo in the database, adding it", e);
            }
        }

        diesel::insert_into(github_repos::table)
            .values(github_repo)
            .get_result(&self.conn())
            .unwrap_or_else(|e| panic!("creating github_repo failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn delete_github_repo_by_name(&self, name: &str) {
        diesel::delete(github_repos::dsl::github_repos.filter(github_repos::dsl::name.eq(name.to_string())))
            .execute(&self.conn())
            .unwrap();
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_journal_club_meetings(&self) -> Vec<JournalClubMeeting> {
        journal_club_meetings::dsl::journal_club_meetings
            .order_by(journal_club_meetings::dsl::id.desc())
            .load::<JournalClubMeeting>(&self.conn())
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_journal_club_meeting(&self, journal_club_meeting: &NewJournalClubMeeting) -> JournalClubMeeting {
        // See if we already have the journal_club_meeting in the database.
        match journal_club_meetings::dsl::journal_club_meetings
            .filter(journal_club_meetings::dsl::issue.eq(journal_club_meeting.issue.to_string()))
            .limit(1)
            .load::<JournalClubMeeting>(&self.conn())
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the journal_club_meeting in the database so we need to add it.
                    // That will happen below.
                } else {
                    let a = r.get(0).unwrap();

                    // Update the journal_club_meeting.
                    return diesel::update(a)
                        .set(journal_club_meeting)
                        .get_result::<JournalClubMeeting>(&self.conn())
                        .unwrap_or_else(|e| panic!("unable to update journal_club_meeting {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the journal_club_meeting in the database, adding it", e);
            }
        }

        diesel::insert_into(journal_club_meetings::table)
            .values(journal_club_meeting)
            .get_result(&self.conn())
            .unwrap_or_else(|e| panic!("creating journal_club_meeting failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_journal_club_papers(&self) -> Vec<JournalClubPaper> {
        journal_club_papers::dsl::journal_club_papers
            .order_by(journal_club_papers::dsl::id.desc())
            .load::<JournalClubPaper>(&self.conn())
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_journal_club_paper(&self, journal_club_paper: &NewJournalClubPaper) -> JournalClubPaper {
        // See if we already have the journal_club_paper in the database.
        match journal_club_papers::dsl::journal_club_papers
            .filter(journal_club_papers::dsl::link.eq(journal_club_paper.link.to_string()))
            .limit(1)
            .load::<JournalClubPaper>(&self.conn())
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the journal_club_paper in the database so we need to add it.
                    // That will happen below.
                } else {
                    let a = r.get(0).unwrap();

                    // Update the journal_club_paper.
                    return diesel::update(a)
                        .set(journal_club_paper)
                        .get_result::<JournalClubPaper>(&self.conn())
                        .unwrap_or_else(|e| panic!("unable to update journal_club_paper {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the journal_club_paper in the database, adding it", e);
            }
        }

        diesel::insert_into(journal_club_papers::table)
            .values(journal_club_paper)
            .get_result(&self.conn())
            .unwrap_or_else(|e| panic!("creating journal_club_paper failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_mailing_list_subscribers(&self) -> Vec<MailingListSubscriber> {
        mailing_list_subscribers::dsl::mailing_list_subscribers
            .order_by(mailing_list_subscribers::dsl::id.desc())
            .load::<MailingListSubscriber>(&self.conn())
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_mailing_list_subscriber(&self, mailing_list_subscriber: &NewMailingListSubscriber) -> MailingListSubscriber {
        // See if we already have the mailing_list_subscriber in the database.
        match mailing_list_subscribers::dsl::mailing_list_subscribers
            .filter(mailing_list_subscribers::dsl::email.eq(mailing_list_subscriber.email.to_string()))
            .limit(1)
            .load::<MailingListSubscriber>(&self.conn())
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the mailing_list_subscriber in the database so we need to add it.
                    // That will happen below.
                } else {
                    let m = r.get(0).unwrap();

                    // Update the mailing_list_subscriber.
                    return diesel::update(m)
                        .set(mailing_list_subscriber)
                        .get_result::<MailingListSubscriber>(&self.conn())
                        .unwrap_or_else(|e| panic!("unable to update mailing_list_subscriber {}: {}", m.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the mailing_list_subscriber in the database, adding it", e);
            }
        }

        diesel::insert_into(mailing_list_subscribers::table)
            .values(mailing_list_subscriber)
            .get_result(&self.conn())
            .unwrap_or_else(|e| panic!("creating mailing_list_subscriber failed: {}", e))
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
