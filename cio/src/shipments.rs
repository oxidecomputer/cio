use std::collections::HashMap;
use std::env;

use async_trait::async_trait;
use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sheets::Sheets;
use shippo::{Address, CustomsDeclaration, CustomsItem, NewShipment, NewTransaction, Parcel, Shippo};
use tracing::instrument;

use crate::airtable::{AIRTABLE_BASE_ID_SHIPMENTS, AIRTABLE_OUTBOUND_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::models::get_value;
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_link: String,
    #[serde(default)]
    pub reprint_label: bool,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub schedule_pickup: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup_date: Option<NaiveDate>,
    pub created_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shipped_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shippo_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub messages: String,
}

impl Shipment {
    #[instrument]
    #[inline]
    fn parse_timestamp(timestamp: &str) -> DateTime<Utc> {
        // Parse the time.
        let time_str = timestamp.to_owned() + " -08:00";
        DateTime::parse_from_str(&time_str, "%m/%d/%Y %H:%M:%S  %:z").unwrap().with_timezone(&Utc)
    }

    /// Parse the sheet columns from single Google Sheets row values.
    /// This is what we get back from the webhook.
    #[instrument]
    #[inline]
    pub fn parse_from_row(values: &HashMap<String, Vec<String>>) -> Self {
        let hoodie_size = get_value(values, "Hoodie");
        let fleece_size = get_value(values, "Fleece");
        let womens_shirt_size = get_value(values, "Women's");
        let unisex_shirt_size = get_value(values, "Unisex");
        let kids_shirt_size = get_value(values, "Onesie");
        let mut contents = String::new();
        if !hoodie_size.is_empty() && !hoodie_size.contains("N/A") {
            contents += &format!("1 x Oxide Hoodie, Size: {}\n", hoodie_size);
        }
        if !fleece_size.is_empty() && !fleece_size.contains("N/A") {
            contents += &format!("1 x Oxide Fleece, Size: {}", fleece_size);
        }
        if !womens_shirt_size.is_empty() && !womens_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Women's Shirt, Size: {}", womens_shirt_size);
        }
        if !unisex_shirt_size.is_empty() && !unisex_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Unisex Shirt, Size: {}", unisex_shirt_size);
        }
        if !kids_shirt_size.is_empty() && !kids_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Kids Shirt, Size: {}", kids_shirt_size);
        }
        Shipment {
            created_time: Shipment::parse_timestamp(&get_value(values, "Timestamp")),
            name: get_value(values, "Name"),
            email: get_value(values, "Email Address"),
            phone: get_value(values, "Phone number"),
            street_1: get_value(values, "Street address line 1"),
            street_2: get_value(values, "Street address line 2"),
            city: get_value(values, "City"),
            state: get_value(values, "State"),
            zipcode: get_value(values, "Zipcode"),
            country: get_value(values, "Country"),
            contents: contents.trim().to_string(),
            carrier: Default::default(),
            pickup_date: None,
            delivered_time: None,
            reprint_label: false,
            schedule_pickup: false,
            shipped_time: None,
            shippo_id: Default::default(),
            status: Default::default(),
            tracking_link: Default::default(),
            tracking_number: Default::default(),
            tracking_status: Default::default(),
            cost: Default::default(),
            label_link: Default::default(),
            eta: None,
            messages: Default::default(),
        }
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
        let mut country = if row.len() > columns.country && columns.country != 0 {
            row[columns.country].trim().to_uppercase()
        } else {
            "US".to_string()
        };
        if country.is_empty() {
            country = "US".to_string();
        }

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
            row[columns.state].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the city column
        // then we have a city.
        let city = if row.len() > columns.city && columns.city != 0 {
            row[columns.city].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the street_1 column
        // then we have a street_1.
        let street_1 = if row.len() > columns.street_1 && columns.street_1 != 0 {
            row[columns.street_1].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the street_2 column
        // then we have a street_2.
        let street_2 = if row.len() > columns.street_2 && columns.street_2 != 0 {
            row[columns.street_2].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the hoodie_size column
        // then we have a hoodie_size.
        let hoodie_size = if row.len() > columns.hoodie_size && columns.hoodie_size != 0 {
            row[columns.hoodie_size].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the fleece_size column
        // then we have a fleece_size.
        let fleece_size = if row.len() > columns.fleece_size && columns.fleece_size != 0 {
            row[columns.fleece_size].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the womens_shirt_size column
        // then we have a womens_shirt_size.
        let womens_shirt_size = if row.len() > columns.womens_shirt_size && columns.womens_shirt_size != 0 {
            row[columns.womens_shirt_size].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the unisex_shirt_size column
        // then we have a unisex_shirt_size.
        let unisex_shirt_size = if row.len() > columns.unisex_shirt_size && columns.unisex_shirt_size != 0 {
            row[columns.unisex_shirt_size].trim().to_uppercase()
        } else {
            "".to_lowercase()
        };

        // If the length of the row is greater than the kids_shirt_size column
        // then we have a kids_shirt_size.
        let kids_shirt_size = if row.len() > columns.kids_shirt_size && columns.kids_shirt_size != 0 {
            row[columns.kids_shirt_size].trim().to_uppercase()
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
        if !womens_shirt_size.is_empty() && !womens_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Women's Shirt, Size: {}", womens_shirt_size);
        }
        if !unisex_shirt_size.is_empty() && !unisex_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Unisex Shirt, Size: {}", unisex_shirt_size);
        }
        if !kids_shirt_size.is_empty() && !kids_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Kids Shirt, Size: {}", kids_shirt_size);
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
                delivered_time: None,
                reprint_label: false,
                schedule_pickup: false,
                shipped_time: None,
                shippo_id: Default::default(),
                status: Default::default(),
                tracking_link: Default::default(),
                tracking_number: Default::default(),
                tracking_status: Default::default(),
                cost: Default::default(),
                label_link: Default::default(),
                eta: None,
                messages: Default::default(),
            },
            sent,
        )
    }

    /// Create or get a shipment in shippo that matches this shipment.
    #[tracing::instrument]
    #[inline]
    pub async fn create_or_get_shippo_shipment(&mut self) {
        // Create the shippo client.
        let shippo_client = Shippo::new_from_env();

        // If we already have a shippo id, get the information for the label.
        if !self.shippo_id.is_empty() {
            let label = shippo_client.get_shipping_label(&self.shippo_id).await.unwrap();

            // Set the additional fields.
            self.tracking_number = label.tracking_number;
            self.tracking_link = label.tracking_url_provider;
            self.tracking_status = label.tracking_status;
            self.label_link = label.label_url;
            self.eta = label.eta;
            self.shippo_id = label.object_id;
            if label.status != "SUCCESS" {
                // Print the messages in the messages field.
                // TODO: make the way it prints more pretty.
                self.messages = format!("{:?}", label.messages);
            }

            // Register a tracking webhook for this shipment.
            let status = shippo_client.register_tracking_webhook(&self.carrier, &self.tracking_number).await.unwrap_or_else(|e| {
                println!("registering the tracking webhook failed: {:?}", e);
                Default::default()
            });

            if self.messages.is_empty() {
                self.messages = status.tracking_status.status_details;
            }

            // Get the status of the shipment.
            if status.tracking_status.status == *"TRANSIT" {
                self.status = "Shipped".to_string();
                // TODO: get the first date it was maked as in transit and use that as the shipped
                // time.
            }
            if status.tracking_status.status == *"DELIVERED" {
                self.status = "Delivered".to_string();
                self.delivered_time = status.tracking_status.status_date;
            }
            if status.tracking_status.status == *"RETURNED" {
                self.status = "Returned".to_string();
            }
            if status.tracking_status.status == *"FAILURE" {
                self.status = "Failure".to_string();
            }

            // Return early.
            return;
        }

        // We need to create the label since we don't have one already.
        let office_phone = "(510) 922-1392".to_string();
        let address_from = Address {
            company: "Oxide Computer Company".to_string(),
            name: "The Oxide Shipping Bot".to_string(),
            street1: "1251 Park Avenue".to_string(),
            city: "Emeryville".to_string(),
            state: "CA".to_string(),
            zip: "94608".to_string(),
            country: "US".to_string(),
            phone: office_phone.to_string(),
            email: "packages@oxide.computer".to_string(),
            is_complete: Default::default(),
            object_id: Default::default(),
            test: Default::default(),
            street2: Default::default(),
            validation_results: Default::default(),
        };

        // If this is an international shipment, we need to define our customs
        // declarations.
        let mut cd: Option<CustomsDeclaration> = None;
        if self.country != "US" {
            let mut cd_inner: CustomsDeclaration = Default::default();
            // Create customs items for each item in our order.
            for line in self.contents.lines() {
                let mut ci: CustomsItem = Default::default();
                ci.description = line.to_string();
                let (prefix, _suffix) = line.split_once(" x ").unwrap();
                // TODO: this will break if more than 9, fix for the future.
                ci.quantity = prefix.parse().unwrap();
                ci.net_weight = "0.25".to_string();
                ci.mass_unit = "lb".to_string();
                ci.value_amount = "100.00".to_string();
                ci.value_currency = "USD".to_string();
                ci.origin_country = "US".to_string();
                let c = shippo_client.create_customs_item(ci).await.unwrap();

                // Add the item to our array of items.
                cd_inner.items.push(c.object_id);
            }

            // Fill out the rest of the customs declaration fields.
            // TODO: make this modifiable.
            cd_inner.certify_signer = "Jess Frazelle".to_string();
            cd_inner.certify = true;
            cd_inner.non_delivery_option = "RETURN".to_string();
            cd_inner.contents_type = "GIFT".to_string();
            cd_inner.contents_explanation = self.contents.to_string();
            // TODO: I think this needs to change for Canada.
            cd_inner.eel_pfc = "NOEEI_30_37_a".to_string();

            // Set the customs declarations.
            cd = Some(cd_inner);
        }

        // We need a phone number for the shipment.
        if self.phone.is_empty() {
            // Use the Oxide office line.
            self.phone = office_phone;
        }

        // Create our shipment.
        let shipment = shippo_client
            .create_shipment(NewShipment {
                address_from,
                address_to: Address {
                    name: self.name.to_string(),
                    street1: self.street_1.to_string(),
                    street2: self.street_2.to_string(),
                    city: self.city.to_string(),
                    state: self.state.to_string(),
                    zip: self.zipcode.to_string(),
                    country: self.country.to_string(),
                    phone: self.phone.to_string(),
                    email: self.email.to_string(),
                    is_complete: Default::default(),
                    object_id: Default::default(),
                    test: Default::default(),
                    company: Default::default(),
                    validation_results: Default::default(),
                },
                parcels: vec![Parcel {
                    metadata: "Default parcel for swag".to_string(),
                    length: "18.75".to_string(),
                    width: "14.5".to_string(),
                    height: "3".to_string(),
                    distance_unit: "in".to_string(),
                    weight: "1".to_string(),
                    mass_unit: "lb".to_string(),
                    object_id: Default::default(),
                    object_owner: Default::default(),
                    object_created: None,
                    object_updated: None,
                    object_state: Default::default(),
                    test: Default::default(),
                }],
                customs_declaration: cd,
            })
            .await
            .unwrap();

        // Now we can create our label from the available rates.
        // Try to find the rate that is "BESTVALUE" or "CHEAPEST".
        for rate in shipment.rates {
            if rate.attributes.contains(&"BESTVALUE".to_string()) || rate.attributes.contains(&"CHEAPEST".to_string()) {
                // Use this rate.
                // Create the shipping label.
                let label = shippo_client
                    .create_shipping_label_from_rate(NewTransaction {
                        rate: rate.object_id,
                        r#async: false,
                        label_file_type: "".to_string(),
                        metadata: "".to_string(),
                    })
                    .await
                    .unwrap();

                // Set the additional fields.
                self.carrier = rate.provider;
                self.cost = rate.amount_local.parse().unwrap();
                self.tracking_number = label.tracking_number.to_string();
                self.tracking_link = label.tracking_url_provider.to_string();
                self.tracking_status = label.tracking_status.to_string();
                self.label_link = label.label_url.to_string();
                self.eta = label.eta;
                self.shippo_id = label.object_id.to_string();
                self.status = "Label created".to_string();
                if label.status != "SUCCESS" {
                    self.status = label.status.to_string();
                    // Print the messages in the messages field.
                    // TODO: make the way it prints more pretty.
                    self.messages = format!("{:?}", label.messages);
                }

                // Register a tracking webhook for this shipment.
                shippo_client.register_tracking_webhook(&self.carrier, &self.tracking_number).await.unwrap_or_else(|e| {
                    println!("registering the tracking webhook failed: {:?}", e);
                    Default::default()
                });

                // Print the label.
                self.print_label().await;
                self.status = "Label printed".to_string();

                break;
            }
        }

        // TODO: do something if we don't find a rate.
        // However we should always find a rate.
    }

    /// Send the label to our printer.
    #[tracing::instrument]
    #[inline]
    pub async fn print_label(&self) {
        let printer_url = env::var("PRINTER_URL").unwrap();
        let client = reqwest::Client::new();
        let resp = client.post(&printer_url).body(json!(self.label_link).to_string()).send().await.unwrap();
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => {
                panic!("[print]: status_code: {}, body: {}", s, resp.text().await.unwrap());
            }
        };
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

    /// Get the row in our airtable workspace.
    #[tracing::instrument]
    #[inline]
    pub async fn get_from_airtable(id: &str) -> Self {
        // Initialize the Airtable client.
        let airtable = airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_SHIPMENTS, "");

        let record: airtable_api::Record<Shipment> = airtable.get_record(AIRTABLE_OUTBOUND_TABLE, id).await.unwrap();

        record.fields
    }
}

/// Implement updating the Airtable record for a Shipment.
#[async_trait]
impl UpdateAirtableRecord<Shipment> for Shipment {
    async fn update_airtable_record(&mut self, record: Shipment) {
        if self.status.is_empty() {
            self.status = record.status;
        }
        if self.carrier.is_empty() {
            self.carrier = record.carrier;
        }
        if self.tracking_number.is_empty() {
            self.tracking_number = record.tracking_number;
        }
        if self.tracking_link.is_empty() {
            self.tracking_link = record.tracking_link;
        }
        if self.tracking_status.is_empty() {
            self.tracking_status = record.tracking_status;
        }
        if self.label_link.is_empty() {
            self.label_link = record.label_link;
        }
        if !self.reprint_label {
            self.reprint_label = record.reprint_label;
        }
        if !self.schedule_pickup {
            self.schedule_pickup = record.schedule_pickup;
        }
        if self.pickup_date.is_none() {
            self.pickup_date = record.pickup_date;
        }
        if self.shipped_time.is_none() {
            self.shipped_time = record.shipped_time;
        }
        if self.delivered_time.is_none() {
            self.delivered_time = record.delivered_time;
        }
        if self.shippo_id.is_empty() {
            self.shippo_id = record.shippo_id;
        }
        if self.eta.is_none() {
            self.eta = record.eta;
        }
        if self.cost == 0.0 {
            self.cost = record.cost;
        }
    }
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
    pub womens_shirt_size: usize,
    pub unisex_shirt_size: usize,
    pub kids_shirt_size: usize,
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
            if c.contains("fleece") {
                columns.fleece_size = index;
            }
            if c.contains("hoodie") {
                columns.hoodie_size = index;
            }
            if c.contains("women's tee") {
                columns.womens_shirt_size = index;
            }
            if c.contains("unisex tee") {
                columns.unisex_shirt_size = index;
            }
            if c.contains("onesie") {
                columns.kids_shirt_size = index;
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

    // Iterate over the Google sheets and get the shipments.
    let mut shipments: Vec<Shipment> = Default::default();
    for sheet_id in get_shipments_spreadsheets() {
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

// Get the sheadsheets that contain shipments.
#[instrument]
#[inline]
pub fn get_shipments_spreadsheets() -> Vec<String> {
    vec!["114nnvYnUq7xuf9dw1pT90OiVpYUE6YfE_pN1wllQuCU".to_string(), "1V2NgYMlNXxxVtp81NLd_bqGllc5aDvSK2ZRqp6n2U-Y".to_string()]
}

// Sync the shipments with airtable.
#[instrument]
#[inline]
pub async fn refresh_airtable_shipments() {
    let shipments = get_google_sheets_shipments().await;

    for mut shipment in shipments {
        shipment.create_or_update_in_airtable().await;
        // Create the shipment in shippo.
        shipment.create_or_get_shippo_shipment().await;
        // Update airtable again.
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
