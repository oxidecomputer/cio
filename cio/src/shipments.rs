use async_trait::async_trait;
use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sheets::Sheets;
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_SHIPMENTS, AIRTABLE_OUTBOUND_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::utils::get_gsuite_token;

/// The data type for a shippo shipment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shipment {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_1: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street_2: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zipcode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    // TODO: make status an enum.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_link: String,
    #[serde(default)]
    pub reprint_label: bool,
    #[serde(default)]
    pub schedule_pickup: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup_date: Option<NaiveDate>,
    pub created_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shipped_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shippo_id: String,
}

impl Shipment {
    #[instrument]
    #[inline]
    fn parse_timestamp(timestamp: &str) -> DateTime<Utc> {
        // Parse the time.
        let time_str = timestamp.to_owned() + " -08:00";
        DateTime::parse_from_str(&time_str, "%m/%d/%Y %H:%M:%S  %:z").unwrap().with_timezone(&Utc)
    }

    /// Parse the shipment from a Google Sheets row, where we also happen to know the columns.
    /// This is how we get the spreadsheet back from the API.
    #[instrument]
    #[inline]
    pub fn parse_from_row_with_columns(columns: &SwagSheetColumns, row: &[String]) -> (Self, bool) {
        // If the length of the row is greater than the sent column
        // then we have a sent status.
        let sent = if row.len() > columns.sent { row[columns.sent].to_lowercase().contains("true") } else { false };

        // If the length of the row is greater than the country column
        // then we have a country.
        let country = if row.len() > columns.country && columns.country != 0 {
            row[columns.country].trim().to_lowercase()
        } else {
            "US".to_string()
        };

        // If the length of the row is greater than the name column
        // then we have a name.
        let name = if row.len() > columns.name && columns.name != 0 {
            row[columns.name].trim().to_string()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the phone column
        // then we have a phone.
        let phone = if row.len() > columns.phone && columns.phone != 0 {
            row[columns.phone].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the zipcode column
        // then we have a zipcode.
        let zipcode = if row.len() > columns.zipcode && columns.zipcode != 0 {
            row[columns.zipcode].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the state column
        // then we have a state.
        let state = if row.len() > columns.state && columns.state != 0 {
            row[columns.state].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the city column
        // then we have a city.
        let city = if row.len() > columns.city && columns.city != 0 {
            row[columns.city].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the street_1 column
        // then we have a street_1.
        let street_1 = if row.len() > columns.street_1 && columns.street_1 != 0 {
            row[columns.street_1].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the street_2 column
        // then we have a street_2.
        let street_2 = if row.len() > columns.street_2 && columns.street_2 != 0 {
            row[columns.street_2].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the hoodie_size column
        // then we have a hoodie_size.
        let hoodie_size = if row.len() > columns.hoodie_size && columns.hoodie_size != 0 {
            row[columns.hoodie_size].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the fleece_size column
        // then we have a fleece_size.
        let fleece_size = if row.len() > columns.fleece_size && columns.fleece_size != 0 {
            row[columns.fleece_size].trim().to_lowercase()
        } else {
            "".to_lowercase()
        };

        let email = row[columns.email].trim().to_string();
        let mut contents = String::new();
        if !hoodie_size.is_empty() && !hoodie_size.contains("N/A") {
            contents += &format!("1 x Oxide Hoodie, Size: {}\n", hoodie_size);
        }
        if !fleece_size.is_empty() && !fleece_size.contains("N/A") {
            contents += &format!("1 x Oxide Fleece, Size: {}", fleece_size);
        }

        (
            Shipment {
                created_time: Shipment::parse_timestamp(&row[columns.timestamp]),
                name,
                email,
                phone,
                street_1,
                street_2,
                city,
                state,
                zipcode,
                country,
                contents: contents.trim().to_string(),
                carrier: Default::default(),
                pickup_date: None,
                received_time: None,
                reprint_label: false,
                schedule_pickup: false,
                shipped_time: None,
                shippo_id: Default::default(),
                status: Default::default(),
                tracking_link: Default::default(),
                tracking_number: Default::default(),
            },
            sent,
        )
    }

    /// Push the row to our Airtable workspace.
    #[tracing::instrument]
    #[inline]
    pub async fn push_to_airtable(&self) {
        // Initialize the Airtable client.
        let airtable = airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_SHIPMENTS, "");

        // Create the record.
        let record = airtable_api::Record {
            id: "".to_string(),
            created_time: None,
            fields: self.clone(),
        };

        // Send the new record to the Airtable client.
        // Batch can only handle 10 at a time.
        let _: Vec<airtable_api::Record<Shipment>> = airtable.create_records(AIRTABLE_OUTBOUND_TABLE, vec![record]).await.unwrap();

        println!("created new row in airtable: {:?}", self);
    }

    /// Update the record in airtable.
    #[tracing::instrument]
    #[inline]
    pub async fn update_in_airtable(&mut self, existing_record: &mut airtable_api::Record<Shipment>) {
        // Initialize the Airtable client.
        let airtable = airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_SHIPMENTS, "");

        // Run the custom trait to update the new record from the old record.
        self.update_airtable_record(existing_record.fields.clone()).await;

        // If the Airtable record and the record that was passed in are the same, then we can return early since
        // we do not need to update it in Airtable.
        // We do this after we update the record so that those fields match as
        // well.
        if self.clone() == existing_record.fields.clone() {
            println!("[airtable] id={} in given object equals Airtable record, skipping update", self.email);
            return;
        }

        existing_record.fields = self.clone();

        airtable.update_records(AIRTABLE_OUTBOUND_TABLE, vec![existing_record.clone()]).await.unwrap();
        println!("[airtable] id={} updated in Airtable", self.email);
    }

    /// Update a row in our airtable workspace.
    #[tracing::instrument]
    #[inline]
    pub async fn create_or_update_in_airtable(&mut self) {
        // Check if we already have the row in Airtable.
        // Initialize the Airtable client.
        let airtable = airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_SHIPMENTS, "");

        let result: Vec<airtable_api::Record<Shipment>> = airtable.list_records(AIRTABLE_OUTBOUND_TABLE, "Grid view", vec![]).await.unwrap();

        let mut records: std::collections::BTreeMap<DateTime<Utc>, airtable_api::Record<Shipment>> = Default::default();
        for record in result {
            records.insert(record.fields.created_time, record);
        }

        for (created_time, record) in records {
            if self.created_time == created_time && self.email == record.fields.email {
                self.update_in_airtable(&mut record.clone()).await;

                return;
            }
        }

        // The record does not exist. We need to create it.
        self.push_to_airtable().await;
    }
}

/// Implement updating the Airtable record for a Shipment.
#[async_trait]
impl UpdateAirtableRecord<Shipment> for Shipment {
    async fn update_airtable_record(&mut self, _record: Shipment) {}
}

/// The data type for a Google Sheet swag columns, we use this when
/// parsing the Google Sheets for shipments.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SwagSheetColumns {
    pub timestamp: usize,
    pub name: usize,
    pub email: usize,
    pub street_1: usize,
    pub street_2: usize,
    pub city: usize,
    pub state: usize,
    pub zipcode: usize,
    pub country: usize,
    pub phone: usize,
    pub sent: usize,
    pub fleece_size: usize,
    pub hoodie_size: usize,
}

impl SwagSheetColumns {
    /// Parse the sheet columns from Google Sheets values.
    #[instrument]
    #[inline]
    pub fn parse(values: &[Vec<String>]) -> Self {
        // Iterate over the columns.
        // TODO: make this less horrible
        let mut columns: SwagSheetColumns = Default::default();

        // Get the first row.
        let row = values.get(0).unwrap();

        for (index, col) in row.iter().enumerate() {
            let c = col.to_lowercase();

            if c.contains("timestamp") {
                columns.timestamp = index;
            }
            if c.contains("name") {
                columns.name = index;
            }
            if c.contains("email address") {
                columns.email = index;
            }
            if c.contains("fleece size") {
                columns.fleece_size = index;
            }
            if c.contains("hoodie size") {
                columns.hoodie_size = index;
            }
            if c.contains("street address line 1") {
                columns.street_1 = index;
            }
            if c.contains("street address line 2") {
                columns.street_2 = index;
            }
            if c.contains("city") {
                columns.city = index;
            }
            if c.contains("state") {
                columns.state = index;
            }
            if c.contains("zipcode") {
                columns.zipcode = index;
            }
            if c.contains("country") {
                columns.country = index;
            }
            if c.contains("phone") {
                columns.phone = index;
            }
            if c.contains("sent") {
                columns.sent = index;
            }
        }
        columns
    }
}

/// Return a vector of all the shipments from Google sheets.
#[instrument]
#[inline]
pub async fn get_google_sheets_shipments() -> Vec<Shipment> {
    // Get the GSuite token.
    let token = get_gsuite_token().await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());

    let swag_sheets = vec!["114nnvYnUq7xuf9dw1pT90OiVpYUE6YfE_pN1wllQuCU"];

    // Iterate over the Google sheets and get the shipments.
    let mut shipments: Vec<Shipment> = Default::default();
    for sheet_id in swag_sheets {
        // Get the values in the sheet.
        let sheet_values = sheets_client.get_values(&sheet_id, "Form Responses 1!A1:S1000".to_string()).await.unwrap();
        let values = sheet_values.values.unwrap();

        if values.is_empty() {
            panic!("unable to retrieve any data values from Google sheet {}", sheet_id);
        }

        // Parse the sheet columns.
        let columns = SwagSheetColumns::parse(&values);

        // Iterate over the rows.
        for (row_index, row) in values.iter().enumerate() {
            if row_index == 0 {
                // Continue the loop since we were on the header row.
                continue;
            } // End get header information.

            // Break the loop early if we reached an empty row.
            if row[columns.email].is_empty() {
                break;
            }

            // Parse the applicant out of the row information.
            let (shipment, sent) = Shipment::parse_from_row_with_columns(&columns, &row);

            if !sent {
                shipments.push(shipment);
            }
        }
    }

    shipments
}

// Sync the shipments with airtable.
#[instrument]
#[inline]
pub async fn refresh_airtable_shipments() {
    let shipments = get_google_sheets_shipments().await;

    for mut shipment in shipments {
        shipment.create_or_update_in_airtable().await;
    }
}

#[cfg(test)]
mod tests {
    use crate::shipments::refresh_airtable_shipments;

    #[ignore]
    #[tokio::test(threaded_scheduler)]
    async fn test_cron_shipments() {
        refresh_airtable_shipments().await;
    }
}
