use std::env;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use tracing::instrument;

use crate::certs::{Certificate, NewCertificate};
use crate::configs::{Building, BuildingConfig, ConferenceRoom, GithubLabel, Group, GroupConfig, LabelConfig, Link, LinkConfig, ResourceConfig, User, UserConfig};
use crate::models::{
    Applicant, AuthUser, AuthUserLogin, GithubRepo, JournalClubMeeting, JournalClubPaper, MailingListSubscriber, NewApplicant, NewAuthUser, NewAuthUserLogin, NewJournalClubMeeting,
    NewJournalClubPaper, NewMailingListSubscriber, NewRFD, NewRepo, RFD,
};
use crate::schema::{
    applicants, auth_user_logins, auth_users, buildings, certificates, conference_rooms, github_labels, github_repos, groups, journal_club_meetings, journal_club_papers, links,
    mailing_list_subscribers, rfds, users,
};

pub struct Database {
    conn: PgConnection,
}

impl Default for Database {
    fn default() -> Self {
        let database_url = env::var("CIO_DATABASE_URL").expect("CIO_DATABASE_URL must be set");

        Database {
            conn: PgConnection::establish(&database_url).unwrap_or_else(|e| panic!("error connecting to {}: {}", database_url, e)),
        }
    }
}

// TODO: more gracefully handle errors
// TODO: possibly generate all this boilerplate as well.
impl Database {
    /// Establish a connection to the database.
    pub fn new() -> Database {
        Default::default()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_applicants(&self) -> Vec<Applicant> {
        applicants::dsl::applicants.order_by(applicants::dsl::id.desc()).load::<Applicant>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_applicant(&self, email: &str, sheet_id: &str) -> Option<Applicant> {
        match applicants::dsl::applicants
            .filter(applicants::dsl::email.eq(email.to_string()))
            .filter(applicants::dsl::sheet_id.eq(sheet_id.to_string()))
            .limit(1)
            .load::<Applicant>(&self.conn)
        {
            Ok(r) => {
                if !r.is_empty() {
                    return Some(r.get(0).unwrap().clone());
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the applicant with email {} and sheet_id {} in the database", email, sheet_id, e);
                return None;
            }
        }

        None
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_applicant(&self, applicant: &NewApplicant) -> Applicant {
        // See if we already have the applicant in the database.
        if let Some(a) = self.get_applicant(&applicant.email, &applicant.sheet_id) {
            // Update the applicant.
            return diesel::update(&a)
                .set(applicant)
                .get_result::<Applicant>(&self.conn)
                .unwrap_or_else(|e| panic!("unable to update applicant {}: {}", a.id, e));
        }

        diesel::insert_into(applicants::table)
            .values(applicant)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating applicant failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn update_applicant(&self, applicant: &Applicant) -> Applicant {
        // Update the applicant.
        diesel::update(applicant)
            .set(applicant.clone())
            .get_result::<Applicant>(&self.conn)
            .unwrap_or_else(|e| panic!("unable to update applicant {}: {}", applicant.id, e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_buildings(&self) -> Vec<Building> {
        buildings::dsl::buildings.order_by(buildings::dsl::id.desc()).load::<Building>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
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
                        .unwrap_or_else(|e| panic!("unable to update building {}: {}", b.id, e));
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

    #[instrument(skip(self))]
    #[inline]
    pub fn get_certificates(&self) -> Vec<Certificate> {
        certificates::dsl::certificates.order_by(certificates::dsl::id.desc()).load::<Certificate>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_certificate(&self, certificate: &NewCertificate) -> Certificate {
        // See if we already have the certificate in the database.
        match certificates::dsl::certificates
            .filter(certificates::dsl::domain.eq(certificate.domain.to_string()))
            .limit(1)
            .load::<Certificate>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the certificate in the database so we need to add it.
                    // That will happen below.
                } else {
                    let b = r.get(0).unwrap();

                    // Update the certificate.
                    return diesel::update(b)
                        .set(certificate)
                        .get_result::<Certificate>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update certificate {}: {}", b.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the certificate in the database, adding it", e);
            }
        }

        diesel::insert_into(certificates::table)
            .values(certificate)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating certificate failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_conference_rooms(&self) -> Vec<ConferenceRoom> {
        conference_rooms::dsl::conference_rooms
            .order_by(conference_rooms::dsl::id.desc())
            .load::<ConferenceRoom>(&self.conn)
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_conference_room(&self, conference_room: &ResourceConfig) -> ConferenceRoom {
        // See if we already have the conference_room in the database.
        match conference_rooms::dsl::conference_rooms
            .filter(conference_rooms::dsl::name.eq(conference_room.name.to_string()))
            .limit(1)
            .load::<ConferenceRoom>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the conference_room in the database so we need to add it.
                    // That will happen below.
                } else {
                    let c = r.get(0).unwrap();

                    // Update the conference_room.
                    return diesel::update(c)
                        .set(conference_room)
                        .get_result::<ConferenceRoom>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update conference_room {}: {}", c.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the conference_room in the database, adding it", e);
            }
        }

        diesel::insert_into(conference_rooms::table)
            .values(conference_room)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating conference_room failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_auth_users(&self) -> Vec<AuthUser> {
        auth_users::dsl::auth_users.order_by(auth_users::dsl::id.desc()).load::<AuthUser>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_auth_user(&self, auth_user: &NewAuthUser) -> AuthUser {
        // See if we already have the auth_user in the database.
        match auth_users::dsl::auth_users
            .filter(auth_users::dsl::user_id.eq(auth_user.user_id.to_string()))
            .limit(1)
            .load::<AuthUser>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the auth_user in the database so we need to add it.
                    // That will happen below.
                } else {
                    let a = r.get(0).unwrap();

                    // Update the auth_user.
                    return diesel::update(a)
                        .set(auth_user)
                        .get_result::<AuthUser>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update auth_user {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the auth_user in the database, adding it", e);
            }
        }

        diesel::insert_into(auth_users::table)
            .values(auth_user)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating auth_user failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_auth_user_logins(&self) -> Vec<AuthUserLogin> {
        auth_user_logins::dsl::auth_user_logins
            .order_by(auth_user_logins::dsl::id.desc())
            .load::<AuthUserLogin>(&self.conn)
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_auth_user_login(&self, auth_user_login: &NewAuthUserLogin) -> AuthUserLogin {
        // See if we already have the auth_user_login in the database.
        match auth_user_logins::dsl::auth_user_logins
            .filter(auth_user_logins::dsl::user_id.eq(auth_user_login.user_id.to_string()))
            .filter(auth_user_logins::dsl::date.eq(auth_user_login.date))
            .limit(1)
            .load::<AuthUserLogin>(&self.conn)
        {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the auth_user_login in the database so we need to add it.
                    // That will happen below.
                } else {
                    let a = r.get(0).unwrap();

                    // Update the auth_user_login.
                    return diesel::update(a)
                        .set(auth_user_login)
                        .get_result::<AuthUserLogin>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update auth_user_login {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the auth_user_login in the database, adding it", e);
            }
        }

        diesel::insert_into(auth_user_logins::table)
            .values(auth_user_login)
            .get_result::<AuthUserLogin>(&self.conn)
            .unwrap_or_else(|e| panic!("creating auth_user_login failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_github_labels(&self) -> Vec<GithubLabel> {
        github_labels::dsl::github_labels.order_by(github_labels::dsl::id.desc()).load::<GithubLabel>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_github_label(&self, github_label: &LabelConfig) -> GithubLabel {
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
                        .unwrap_or_else(|e| panic!("unable to update github_label {}: {}", label.id, e));
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

    #[instrument(skip(self))]
    #[inline]
    pub fn get_github_repos(&self) -> Vec<GithubRepo> {
        github_repos::dsl::github_repos.order_by(github_repos::dsl::id.desc()).load::<GithubRepo>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_github_repo(&self, github_repo: &NewRepo) -> GithubRepo {
        // See if we already have the github_repo in the database.
        match github_repos::dsl::github_repos
            .filter(github_repos::dsl::full_name.eq(github_repo.full_name.to_string()))
            .limit(1)
            .load::<GithubRepo>(&self.conn)
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
                        .get_result::<GithubRepo>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update github_repo {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the github_repo in the database, adding it", e);
            }
        }

        diesel::insert_into(github_repos::table)
            .values(github_repo)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating github_repo failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn delete_github_repo_by_name(&self, name: &str) {
        diesel::delete(github_repos::dsl::github_repos.filter(github_repos::dsl::name.eq(name.to_string())))
            .execute(&self.conn)
            .unwrap();
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_groups(&self) -> Vec<Group> {
        groups::dsl::groups.order_by(groups::dsl::id.desc()).load::<Group>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_group(&self, group: &GroupConfig) -> Group {
        // See if we already have the group in the database.
        match groups::dsl::groups.filter(groups::dsl::name.eq(group.name.to_string())).limit(1).load::<Group>(&self.conn) {
            Ok(r) => {
                if r.is_empty() {
                    // We don't have the group in the database so we need to add it.
                    // That will happen below.
                } else {
                    let g = r.get(0).unwrap();

                    // Update the group.
                    return diesel::update(g)
                        .set(group)
                        .get_result::<Group>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update group {}: {}", g.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the group in the database, adding it", e);
            }
        }

        diesel::insert_into(groups::table)
            .values(group)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating group failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_journal_club_meetings(&self) -> Vec<JournalClubMeeting> {
        journal_club_meetings::dsl::journal_club_meetings
            .order_by(journal_club_meetings::dsl::id.desc())
            .load::<JournalClubMeeting>(&self.conn)
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_journal_club_meeting(&self, journal_club_meeting: &NewJournalClubMeeting) -> JournalClubMeeting {
        // See if we already have the journal_club_meeting in the database.
        match journal_club_meetings::dsl::journal_club_meetings
            .filter(journal_club_meetings::dsl::issue.eq(journal_club_meeting.issue.to_string()))
            .limit(1)
            .load::<JournalClubMeeting>(&self.conn)
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
                        .get_result::<JournalClubMeeting>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update journal_club_meeting {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the journal_club_meeting in the database, adding it", e);
            }
        }

        diesel::insert_into(journal_club_meetings::table)
            .values(journal_club_meeting)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating journal_club_meeting failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_journal_club_papers(&self) -> Vec<JournalClubPaper> {
        journal_club_papers::dsl::journal_club_papers
            .order_by(journal_club_papers::dsl::id.desc())
            .load::<JournalClubPaper>(&self.conn)
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_journal_club_paper(&self, journal_club_paper: &NewJournalClubPaper) -> JournalClubPaper {
        // See if we already have the journal_club_paper in the database.
        match journal_club_papers::dsl::journal_club_papers
            .filter(journal_club_papers::dsl::link.eq(journal_club_paper.link.to_string()))
            .limit(1)
            .load::<JournalClubPaper>(&self.conn)
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
                        .get_result::<JournalClubPaper>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update journal_club_paper {}: {}", a.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the journal_club_paper in the database, adding it", e);
            }
        }

        diesel::insert_into(journal_club_papers::table)
            .values(journal_club_paper)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating journal_club_paper failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_links(&self) -> Vec<Link> {
        links::dsl::links.order_by(links::dsl::id.desc()).load::<Link>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_link(&self, link: &LinkConfig) -> Link {
        // See if we already have the link in the database.
        match links::dsl::links.filter(links::dsl::name.eq(link.name.to_string())).limit(1).load::<Link>(&self.conn) {
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
                        .unwrap_or_else(|e| panic!("unable to update link {}: {}", l.id, e));
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

    #[instrument(skip(self))]
    #[inline]
    pub fn get_mailing_list_subscribers(&self) -> Vec<MailingListSubscriber> {
        mailing_list_subscribers::dsl::mailing_list_subscribers
            .order_by(mailing_list_subscribers::dsl::id.desc())
            .load::<MailingListSubscriber>(&self.conn)
            .unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_mailing_list_subscriber(&self, mailing_list_subscriber: &NewMailingListSubscriber) -> MailingListSubscriber {
        // See if we already have the mailing_list_subscriber in the database.
        match mailing_list_subscribers::dsl::mailing_list_subscribers
            .filter(mailing_list_subscribers::dsl::email.eq(mailing_list_subscriber.email.to_string()))
            .limit(1)
            .load::<MailingListSubscriber>(&self.conn)
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
                        .get_result::<MailingListSubscriber>(&self.conn)
                        .unwrap_or_else(|e| panic!("unable to update mailing_list_subscriber {}: {}", m.id, e));
                }
            }
            Err(e) => {
                println!("[db] on err: {:?}; we don't have the mailing_list_subscriber in the database, adding it", e);
            }
        }

        diesel::insert_into(mailing_list_subscribers::table)
            .values(mailing_list_subscriber)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating mailing_list_subscriber failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_rfds(&self) -> Vec<RFD> {
        rfds::dsl::rfds.order_by(rfds::dsl::id.desc()).load::<RFD>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_rfd(&self, number: i32) -> Option<RFD> {
        match rfds::dsl::rfds.filter(rfds::dsl::number.eq(number)).limit(1).load::<RFD>(&self.conn) {
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
                .get_result::<RFD>(&self.conn)
                .unwrap_or_else(|e| panic!("unable to update rfd {}: {}", r.id, e));
        }

        diesel::insert_into(rfds::table)
            .values(rfd)
            .get_result(&self.conn)
            .unwrap_or_else(|e| panic!("creating rfd failed: {}", e))
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn get_users(&self) -> Vec<User> {
        users::dsl::users.order_by(users::dsl::id.desc()).load::<User>(&self.conn).unwrap()
    }

    #[instrument(skip(self))]
    #[inline]
    pub fn upsert_user(&self, user: &UserConfig) -> User {
        // See if we already have the user in the database.
        match users::dsl::users.filter(users::dsl::username.eq(user.username.to_string())).limit(1).load::<User>(&self.conn) {
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
                        .unwrap_or_else(|e| panic!("unable to update user {}: {}", u.id, e));
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
