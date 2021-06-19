/*use std::collections::HashMap;
use std::env;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::companies::Company;
use crate::db::Database;
use crate::mailing_list::NewMailingListSubscriber;
use crate::rack_line::NewRackLineSubscriber;


impl MailchimpWebhook {
    /// Convert to a signup data type.
    pub fn as_mailing_list_subscriber(&self, db: &Database) -> NewMailingListSubscriber {
        let mut signup: NewMailingListSubscriber = Default::default();

        let list_id = self.data.list_id.as_ref().unwrap();

        // Get the company from the list id.
        let company = Company::get_from_mailchimp_list_id(db, &list_id);

        if self.data.merges.is_some() {
            let merges = self.data.merges.as_ref().unwrap();

            if let Some(e) = &merges.email {
                signup.email = e.trim().to_string();
            }
            if let Some(f) = &merges.first_name {
                signup.first_name = f.trim().to_string();
            }
            if let Some(l) = &merges.last_name {
                signup.last_name = l.trim().to_string();
            }
            if let Some(c) = &merges.company {
                signup.company = c.trim().to_string();
            }
            if let Some(i) = &merges.interest {
                signup.interest = i.trim().to_string();
            }

            if merges.groupings.is_some() {
                let groupings = merges.groupings.as_ref().unwrap();

                signup.wants_podcast_updates = groupings[0].groups.is_some();
                signup.wants_newsletter = groupings[1].groups.is_some();
                signup.wants_product_updates = groupings[2].groups.is_some();
            }
        }

        signup.date_added = self.fired_at;
        signup.date_optin = self.fired_at;
        signup.date_last_changed = self.fired_at;
        signup.name = format!("{} {}", signup.first_name, signup.last_name);

        signup.cio_company_id = company.id;

        signup
    }

    /// Convert to a signup data type.
    pub fn as_rack_line_subscriber(&self, db: &Database) -> NewRackLineSubscriber {
        let mut signup: NewRackLineSubscriber = Default::default();

        let list_id = self.data.list_id.as_ref().unwrap();

        // Get the company from the list id.
        let company = Company::get_from_mailchimp_list_id(db, &list_id);

        if self.data.merges.is_some() {
            let merges = self.data.merges.as_ref().unwrap();

            if let Some(e) = &merges.email {
                signup.email = e.trim().to_string();
            }
            if let Some(f) = &merges.name {
                signup.name = f.trim().to_string();
            }
            if let Some(c) = &merges.company {
                signup.company = c.trim().to_string();
            }
            if let Some(c) = &merges.company_size {
                signup.company_size = c.trim().to_string();
            }
            if let Some(i) = &merges.notes {
                signup.interest = i.trim().to_string();
            }
        }

        signup.date_added = self.fired_at;
        signup.date_optin = self.fired_at;
        signup.date_last_changed = self.fired_at;

        signup.cio_company_id = company.id;

        signup
    }
}*/
