use std::collections::BTreeMap;
use std::env;
use std::str::from_utf8;

use chrono::naive::NaiveTime;
use chrono::Utc;
use handlebars::Handlebars;
use log::{info, warn};
use serde_json;
use tokio::runtime::Runtime;

use crate::airtable::client::Airtable;
use crate::core::{DiscussionFields, MeetingFields, ProductEmailData};
use crate::email::client::SendGrid;
use crate::utils::authenticate_github;

pub static DISCUSSION_TOPICS_TABLE: &'static str = "Discussion topics";
pub static MEETING_SCHEDULE_TABLE: &'static str = "Meeting schedule";

// TODO: automatically add notes to the reports repo.
// TODO: make this a cron job
// TODO: test when there are actually topics
// TODO: send out last meetings notes in the email as well with the link to the reports repo
pub fn cmd_product_huddle_run() {
    // Initialize the Airtable client.
    let airtable = Airtable::new_from_env();

    // Initialize Github and the runtime.
    let mut runtime = Runtime::new().unwrap();
    let github_org = env::var("GITHUB_ORG").unwrap();
    let github = authenticate_github();
    // Get the reports repo client.
    let reports_repo = github.repo(github_org, "reports");

    // Get the meeting schedule table from airtable.
    let records_ms = airtable
        .list_records(MEETING_SCHEDULE_TABLE, "All Meetings")
        .unwrap();

    let date_format = "%A, %-d %B, %C%y";

    // Iterate over the airtable records and update the RFD where we have one.
    // Add them to a BTreeMap so we can easily access them later.
    let mut meetings: BTreeMap<String, MeetingFields> = Default::default();
    for (_i, record) in records_ms.clone().iter().enumerate() {
        // Deserialize the fields.
        // TODO: find a nicer way to do this.
        let meeting: MeetingFields = serde_json::from_value(record.fields.clone()).unwrap();

        meetings.insert(record.clone().id.unwrap(), meeting.clone());

        // Check if we have the meeting notes in the reports repo.
        match &meeting.notes {
            None => (),
            Some(raw) => {
                if raw.len() > 0 {
                    let notes_path = format!(
                        "/product/meetings/{}.txt",
                        meeting.date.format("%Y%m%d").to_string()
                    );

                    let notes = format!(
                        "# Product Huddle on {}\n\n## Notes\n\n{}\n\n## Action Items\n\n{}",
                        meeting.clone().date.format(date_format),
                        raw,
                        meeting.action_items.unwrap_or(
                            "There were no action items as a result of this meeting".to_string()
                        ),
                    );

                    // Try to get the notes from this meeting from the reports repo.
                    match runtime.block_on(reports_repo.content().file(&notes_path)) {
                        Ok(file) => {
                            let decoded = from_utf8(&file.content).unwrap();
                            // Compare the notes and see if we need to update them.
                            if notes == decoded {
                                // They are the same so we can continue through the loop.
                                continue;
                            }

                            // We need to update the file.
                            info!(
                                "Updated the notes file in the reports repo at {}",
                                notes_path
                            );
                        }
                        Err(_) => {
                            // Create the notes file in the repo.
                            runtime
                                .block_on(reports_repo.content().create(&notes_path, &notes))
                                .unwrap();

                            info!(
                                "Created the notes file in the reports repo at {}",
                                notes_path
                            );
                        }
                    }
                }
            }
        }
    }

    // Get the current discussion list from airtable.
    let records_dt = airtable
        .list_records(DISCUSSION_TOPICS_TABLE, "Proposed Topics")
        .unwrap();

    // Get the time now.
    let now = Utc::now().naive_utc();
    // Create our discussion topics array.
    let mut email_data: ProductEmailData = Default::default();

    // Iterate over the airtable records and update the RFD where we have one.
    for (_i, record) in records_dt.clone().iter().enumerate() {
        // Deserialize the fields.
        // TODO: find a nicer way to do this.
        let fields: DiscussionFields = serde_json::from_value(record.fields.clone()).unwrap();

        // Get the meetings that match this discussion topic.
        for meeting_id in &fields.associated_meetings {
            match meetings.get(meeting_id) {
                Some(meeting) => {
                    // Check if the meeting is in the future or the past.
                    // 18 is 11am Pacific Time in UTC time.
                    let date = meeting.date.and_time(NaiveTime::from_hms(18, 0, 0));
                    // Compare the dates.
                    let dur = date.signed_duration_since(now);
                    if dur.num_seconds() < 0 {
                        // The meeting happened in the past.
                        // Continue since it is not relevant unless it is in the
                        // future.
                        continue;
                    }

                    // If we are here then our date is in the future.
                    if dur.num_days() < 7 {
                        email_data.date = meeting.date.format(date_format).to_string();
                        // Add it to our list for the email.
                        email_data.topics.push(fields);
                        break;
                    }
                }
                None => warn!("could not find matching meeting for id: {}", meeting_id),
            }
        }
    }

    // Initialize handlebars.
    let handlebars = Handlebars::new();
    // Render the template.
    let template = &handlebars
        .render_template(EMAIL_TEMPLATE, &email_data)
        .unwrap();
    println!("{}", template);

    // Initialize the SendGrid client.
    let sendgrid = SendGrid::new_from_env();
    // Send the email.
    sendgrid.send(
        template,
        "Reminder Product Huddle Tomorrow",
        "product@oxide.computer",
        "jess@oxide.computer",
    );

    info!("successfully sent the email!")
}

pub static EMAIL_TEMPLATE: &'static str = r#"Hello All!!

This is your weekly automated reminder that tomorrow is the regularly scheduled
product huddle meeting. As usual, discussion topics are submitted through the
Airtable form: https://product-huddle-form.corp.oxide.computer.

Get your discussion topics into the form before tomorrow!{{#if this.topics}} The following
dicussion topics have already been proposed, but it is not too late to add
something you have been nooding on as well.

# Discussion topics for {{this.date}}

{{#each this.topics}}
- Topic: {{this.topic}}
  Submitted by: {{this.submitter.name}}
  Priority: {{this.priority}}
  Notes: {{this.notes}}

{{/each}}{{else}} There are no topics
to discuss yet! If no one submits any before the meeting tomorrow, we will cancel
the meeting. So get your topics in!!
{{/if}}

As a friendly reminder, the product huddle meetings are stored in Airtable at:
https://airtable-product-huddle.corp.oxide.computer.

The product roadmap is also in Airtable at: https://airtable-roadmap.corp.oxide.computer.

See you soon,

The Oxide Product Team Bot
"#;
