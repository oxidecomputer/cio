use log::{info, warn};
use serde_json;

use airtable::{Airtable, Record};

use crate::core::{RFDFields, RFD};
use crate::utils::{authenticate_github, get_rfds_from_repo};

pub static RFD_TABLE: &str = "RFDs";

/// Sync airtable with our RFDs in GitHub.
pub fn cmd_airtable_run() {
    // Initialize the Airtable client.
    let airtable = Airtable::new_from_env();

    // Initialize Github and the runtime.
    let github = authenticate_github();

    // Get the rfds from our the repo.
    let mut rfds = get_rfds_from_repo(github);

    // Get the current RFD list from airtable.
    let mut records = airtable.list_records(RFD_TABLE, "Grid view").unwrap();

    // Iterate over the airtable records and update the RFD where we have one.
    for (i, record) in records.clone().iter().enumerate() {
        // Deserialize the fields.
        // TODO: find a nicer way to do this.
        let mut fields: RFDFields =
            serde_json::from_value(record.fields.clone()).unwrap();

        // Try to find the matching RFD.
        let rfd: RFD;
        match rfds.get(&fields.number) {
            Some(val) => rfd = val.clone(),
            // Warn because we should probably delete the RFD in airtable.
            // TODO: delete the not found RFD in airtable.
            None => {
                warn!("could not find RFD {} in our CSV list", fields.number);
                // Continue for now.
                continue;
            }
        }

        // Update the RFD in airtable.
        fields.title = rfd.title.to_string();
        fields.state = rfd.state.to_string();
        // Set the computed values back to empty since these are functions in airtable.
        fields.link = None;
        fields.name = None;
        records[i].fields = serde_json::to_value(fields.clone()).unwrap();

        // Send the updated record to the airtable client.
        // Batch can only handle 10 at a time.
        // TODO: find a way to make this more efficient by doing 10 at a time.
        airtable
            .update_records(RFD_TABLE, vec![records[i].clone()])
            .unwrap();

        // Remove the rfd from our rfds BTreeMap so we all we are left with are
        // the RFDs to be created.
        rfds.remove(&fields.number);

        info!("updated record for RFD {}", fields.number);
    }

    // Create any new RFD records.
    for (num, rfd) in rfds {
        // Create the record fields.
        let fields = RFDFields {
            number: num,
            title: rfd.title,
            state: rfd.state,
            link: None,
            name: None,
        };

        // Create the record.
        let record = Record {
            id: None,
            created_time: None,
            fields: serde_json::to_value(fields.clone()).unwrap(),
        };

        // Send the record to airtable.
        // TODO: do this in bulk
        airtable.create_records(RFD_TABLE, vec![record]).unwrap();

        info!("created record for RFD {}", fields.number);
    }
}
