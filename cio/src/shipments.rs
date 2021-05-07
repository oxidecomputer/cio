#![allow(clippy::from_over_into)]
use std::collections::HashMap;
use std::convert::From;
use std::env;

use async_trait::async_trait;
use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::DateTime;
use macros::db;
use reqwest::StatusCode;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};
use sheets::Sheets;
use shippo::{Address, CustomsDeclaration, CustomsItem, NewShipment, NewTransaction, Parcel, Shippo};

use crate::airtable::{AIRTABLE_BASE_ID_SHIPMENTS, AIRTABLE_INBOUND_TABLE, AIRTABLE_OUTBOUND_TABLE};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::models::get_value;
use crate::schema::{inbound_shipments, outbound_shipments};
use crate::utils::{get_gsuite_token, DOMAIN};

/// The data type for an inbound shipment.
#[db {
    new_struct_name = "InboundShipment",
    airtable_base_id = "AIRTABLE_BASE_ID_SHIPMENTS",
    airtable_table = "AIRTABLE_INBOUND_TABLE",
    match_on = {
        "tracking_number" = "String",
        "carrier" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "inbound_shipments"]
pub struct NewInboundShipment {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oxide_tracking_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shipped_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub messages: String,

    /// These fields are filled in by the Airtable and should not be edited by the
    /// API updating.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// Implement updating the Airtable record for an InboundShipment.
#[async_trait]
impl UpdateAirtableRecord<InboundShipment> for InboundShipment {
    async fn update_airtable_record(&mut self, record: InboundShipment) {
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
        if self.shipped_time.is_none() {
            self.shipped_time = record.shipped_time;
        }
        if self.delivered_time.is_none() {
            self.delivered_time = record.delivered_time;
        }
        if self.eta.is_none() {
            self.eta = record.eta;
        }
        if self.notes.is_empty() {
            self.notes = record.notes;
        }
    }
}

impl NewInboundShipment {
    pub fn oxide_tracking_link(&self) -> String {
        format!("https://track.oxide.computer/{}/{}", self.carrier, self.tracking_number)
    }

    // Get the tracking link for the provider.
    fn tracking_link(&mut self) {
        let carrier = self.carrier.to_lowercase();

        if carrier == "usps" {
            self.tracking_link = format!("https://tools.usps.com/go/TrackConfirmAction_input?origTrackNum={}", self.tracking_number);
        } else if carrier == "ups" {
            self.tracking_link = format!("https://www.ups.com/track?tracknum={}", self.tracking_number);
        } else if carrier == "fedex" {
            self.tracking_link = format!("https://www.fedex.com/apps/fedextrack/?tracknumbers={}", self.tracking_number);
        } else if carrier == "dhl" {
            // TODO: not sure if this one is correct.
            self.tracking_link = format!("https://www.dhl.com/en/express/tracking.html?AWB={}", self.tracking_number);
        }
    }

    /// Get the details about the shipment from the tracking API.
    pub async fn expand(&mut self) {
        // Create the shippo client.
        let shippo = Shippo::new_from_env();

        let mut carrier = self.carrier.to_lowercase().to_string();
        if carrier == "dhl" {
            carrier = "dhl_express".to_string();
        }

        // Get the tracking status for the shipment and fill in the details.
        let ts = shippo.get_tracking_status(&carrier, &self.tracking_number).await.unwrap_or_default();
        self.tracking_number = ts.tracking_number.to_string();
        let status = ts.tracking_status.unwrap_or_default();
        self.tracking_status = status.status.to_string();
        self.tracking_link();
        self.eta = ts.eta;

        self.oxide_tracking_link = self.oxide_tracking_link();

        self.messages = status.status_details;

        // Iterate over the tracking history and set the shipped_time.
        // Get the first date it was maked as in transit and use that as the shipped
        // time.
        for h in ts.tracking_history {
            if h.status == *"TRANSIT" {
                if let Some(shipped_time) = h.status_date {
                    let current_shipped_time = if let Some(s) = self.shipped_time { s } else { Utc::now() };

                    if shipped_time < current_shipped_time {
                        self.shipped_time = Some(shipped_time);
                    }
                }
            }
        }

        if status.status == *"DELIVERED" {
            self.delivered_time = status.status_date;
        }
    }
}

impl InboundShipment {
    pub fn oxide_tracking_link(&self) -> String {
        format!("https://track.oxide.computer/{}/{}", self.carrier, self.tracking_number)
    }

    // Get the tracking link for the provider.
    pub fn tracking_link(&mut self) {
        let carrier = self.carrier.to_lowercase();

        if carrier == "usps" {
            self.tracking_link = format!("https://tools.usps.com/go/TrackConfirmAction_input?origTrackNum={}", self.tracking_number);
        } else if carrier == "ups" {
            self.tracking_link = format!("https://www.ups.com/track?tracknum={}", self.tracking_number);
        } else if carrier == "fedex" {
            self.tracking_link = format!("https://www.fedex.com/apps/fedextrack/?tracknumbers={}", self.tracking_number);
        } else if carrier == "dhl" {
            // TODO: not sure if this one is correct.
            self.tracking_link = format!("https://www.dhl.com/en/express/tracking.html?AWB={}", self.tracking_number);
        }
    }
}

/// The data type for an outbound shipment.
#[db {
    new_struct_name = "OutboundShipment",
    airtable_base_id = "AIRTABLE_BASE_ID_SHIPMENTS",
    airtable_table = "AIRTABLE_OUTBOUND_TABLE",
    match_on = {
        "tracking_number" = "String",
        "carrier" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "outbound_shipments"]
pub struct NewOutboundShipment {
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
    pub address_formatted: String,
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
    pub oxide_tracking_link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_link: String,
    #[serde(default)]
    pub reprint_label: bool,
    #[serde(default)]
    pub resend_email_to_recipient: bool,
    #[serde(default)]
    pub cost: f32,
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub geocode_cache: String,
}

impl NewOutboundShipment {
    fn parse_timestamp(timestamp: &str) -> DateTime<Utc> {
        // Parse the time.
        let time_str = timestamp.to_owned() + " -08:00";
        DateTime::parse_from_str(&time_str, "%m/%d/%Y %H:%M:%S  %:z").unwrap().with_timezone(&Utc)
    }

    /// Parse the sheet columns from single Google Sheets row values.
    /// This is what we get back from the webhook.
    pub fn parse_from_row(values: &HashMap<String, Vec<String>>) -> Self {
        let hoodie_size = get_value(values, "Hoodie");
        let fleece_size = get_value(values, "Patagonia Fleece");
        let womens_shirt_size = get_value(values, "Women's Tee");
        let unisex_shirt_size = get_value(values, "Unisex Tee");
        let kids_shirt_size = get_value(values, "Onesie / Toddler / Youth Sizes");
        let mut contents = String::new();
        if !hoodie_size.is_empty() && !hoodie_size.contains("N/A") {
            contents += &format!("1 x Oxide Hoodie, Size: {}\n", hoodie_size);
        }
        if !fleece_size.is_empty() && !fleece_size.contains("N/A") {
            contents += &format!("1 x Oxide Fleece, Size: {}\n", fleece_size);
        }
        if !womens_shirt_size.is_empty() && !womens_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Women's Shirt, Size: {}\n", womens_shirt_size);
        }
        if !unisex_shirt_size.is_empty() && !unisex_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Unisex Shirt, Size: {}\n", unisex_shirt_size);
        }
        if !kids_shirt_size.is_empty() && !kids_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Kids Shirt, Size: {}\n", kids_shirt_size);
        }

        let mut country = get_value(values, "Country");
        if country.is_empty() {
            country = "US".to_string();
        }
        NewOutboundShipment {
            created_time: NewOutboundShipment::parse_timestamp(&get_value(values, "Timestamp")),
            name: get_value(values, "Name"),
            email: get_value(values, "Email Address").to_lowercase(),
            phone: get_value(values, "Phone number"),
            street_1: get_value(values, "Street address line 1").to_uppercase(),
            street_2: get_value(values, "Street address line 2").to_uppercase(),
            city: get_value(values, "City").to_uppercase(),
            state: get_value(values, "State").to_uppercase(),
            zipcode: get_value(values, "Zipcode").to_uppercase(),
            country,
            address_formatted: String::new(),
            contents: contents.trim().to_string(),
            carrier: Default::default(),
            pickup_date: None,
            delivered_time: None,
            reprint_label: false,
            schedule_pickup: false,
            resend_email_to_recipient: false,
            shipped_time: None,
            shippo_id: Default::default(),
            status: "Queued".to_string(),
            tracking_link: Default::default(),
            oxide_tracking_link: Default::default(),
            tracking_number: Default::default(),
            tracking_status: Default::default(),
            cost: Default::default(),
            label_link: Default::default(),
            eta: None,
            messages: Default::default(),
            notes: Default::default(),
            geocode_cache: Default::default(),
        }
    }

    /// Parse the shipment from a Google Sheets row, where we also happen to know the columns.
    /// This is how we get the spreadsheet back from the API.
    pub fn parse_from_row_with_columns(db: &Database, columns: &SwagSheetColumns, row: &[String]) -> (Self, bool) {
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
            row[columns.zipcode].trim().to_uppercase()
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

        // TODO: make all these more DRY.
        let email = row[columns.email].trim().to_lowercase();
        let mut contents = String::new();
        if !hoodie_size.is_empty() && !hoodie_size.contains("N/A") {
            contents += &format!("1 x Oxide Hoodie, Size: {}\n", hoodie_size);
        }
        if !fleece_size.is_empty() && !fleece_size.contains("N/A") {
            contents += &format!("1 x Oxide Fleece, Size: {}\n", fleece_size);
        }
        if !womens_shirt_size.is_empty() && !womens_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Women's Shirt, Size: {}\n", womens_shirt_size);
        }
        if !unisex_shirt_size.is_empty() && !unisex_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Unisex Shirt, Size: {}\n", unisex_shirt_size);
        }
        if !kids_shirt_size.is_empty() && !kids_shirt_size.contains("N/A") {
            contents += &format!("1 x Oxide Kids Shirt, Size: {}\n", kids_shirt_size);
        }

        let created_time = NewOutboundShipment::parse_timestamp(&row[columns.timestamp]);

        let mut carrier = Default::default();
        let mut address_formatted = Default::default();
        let mut shippo_id = Default::default();
        let mut pickup_date = Default::default();
        let mut delivered_time = Default::default();
        let mut reprint_label = Default::default();
        let mut schedule_pickup = Default::default();
        let mut resend_email_to_recipient = Default::default();
        let mut shipped_time = Default::default();
        let mut tracking_link = Default::default();
        let mut oxide_tracking_link = Default::default();
        let mut cost = Default::default();
        let mut tracking_status = Default::default();
        let mut label_link = Default::default();
        let mut eta = Default::default();
        let mut notes = Default::default();
        let mut geocode_cache = Default::default();
        let mut messages = Default::default();
        let mut status = Default::default();
        let mut tracking_number = Default::default();

        // Let's try to get the record from the database.
        if let Ok(shipment) = outbound_shipments::dsl::outbound_shipments
            .filter(outbound_shipments::dsl::email.eq(email.to_string()))
            .filter(outbound_shipments::dsl::created_time.eq(created_time))
            .first::<OutboundShipment>(&db.conn())
        {
            // Let's set some other fields.
            carrier = shipment.carrier.to_string();
            address_formatted = shipment.address_formatted.to_string();
            shippo_id = shipment.shippo_id.to_string();
            pickup_date = shipment.pickup_date;
            delivered_time = shipment.delivered_time;
            reprint_label = shipment.reprint_label;
            schedule_pickup = shipment.schedule_pickup;
            resend_email_to_recipient = shipment.resend_email_to_recipient;
            shipped_time = shipment.shipped_time;
            tracking_link = shipment.tracking_link.to_string();
            oxide_tracking_link = shipment.oxide_tracking_link.to_string();
            cost = shipment.cost;
            tracking_status = shipment.tracking_status.to_string();
            label_link = shipment.label_link.to_string();
            eta = shipment.eta;
            notes = shipment.notes.to_string();
            geocode_cache = shipment.geocode_cache.to_string();
            messages = shipment.messages.to_string();
            status = shipment.status.to_string();
            tracking_number = shipment.tracking_number;
        }

        (
            NewOutboundShipment {
                created_time,
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
                address_formatted,
                carrier,
                pickup_date,
                delivered_time,
                reprint_label,
                schedule_pickup,
                resend_email_to_recipient,
                shipped_time,
                shippo_id,
                status,
                tracking_link,
                oxide_tracking_link,
                tracking_number,
                tracking_status,
                cost,
                label_link,
                eta,
                messages,
                notes,
                geocode_cache,
            },
            sent,
        )
    }
}

/// Implement updating the Airtable record for an OutboundShipment.
#[async_trait]
impl UpdateAirtableRecord<OutboundShipment> for OutboundShipment {
    async fn update_airtable_record(&mut self, record: OutboundShipment) {
        self.geocode_cache = record.geocode_cache;

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
        if self.notes.is_empty() {
            self.notes = record.notes;
        }
    }
}

impl OutboundShipment {
    fn populate_formatted_address(&mut self) {
        let mut street_address = self.street_1.to_string();
        if !self.street_2.is_empty() {
            street_address = format!("{}\n{}", self.street_1, self.street_2,);
        }
        self.address_formatted = format!("{}\n{}, {} {} {}", street_address, self.city, self.state, self.zipcode, self.country)
            .trim()
            .trim_matches(',')
            .trim()
            .to_string();
    }

    pub fn oxide_tracking_link(&self) -> String {
        format!("https://track.oxide.computer/{}/{}", self.carrier, self.tracking_number)
    }

    /// Send the label to our printer.
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

    /// Format address.
    pub fn format_address(&self) -> String {
        let mut street = self.street_1.to_string();
        if !self.street_2.is_empty() {
            street = format!("{}\n{}", self.street_1, self.street_2);
        }

        format!("{}\n{}, {} {} {}", street, self.city, self.state, self.zipcode, self.country)
    }

    /// Send an email to the recipient with their tracking code and information.
    pub async fn send_email_to_recipient(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .send_mail(
                "Your package from the Oxide Computer Company is on the way!".to_string(),
                format!(
                    "Below is the information for your package:

**Contents:**
{}

**Address to:**
{}
{}

**Tracking link:**
{}

If you have any questions or concerns, please respond to this email!
Have a splendid day!

xoxo,
  The Oxide Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                    self.oxide_tracking_link
                ),
                vec![self.email.to_string()],
                vec![],
                vec![],
                format!("packages@{}", DOMAIN),
            )
            .await;
    }

    /// Send an email internally that we need to package the shipment.
    pub async fn send_email_internally(&self) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .send_mail(
                format!("Shipment to {} is ready to be packaged", self.name),
                format!(
                    "Below is the information the package:

**Contents:**
{}

**Address to:**
{}
{}

**Tracking link:**
{}

The label should already be printed in the big conference room. Please take the
label and affix it to the package with the specified contents. It can then be dropped off
for {}.

As always, the Airtable with all the shipments lives at:
https://airtable-shipments.corp.oxide.computer.

xoxo,
  The Oxide Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                    self.oxide_tracking_link,
                    self.carrier,
                ),
                vec![format!("packages@{}", DOMAIN)],
                vec![],
                vec![],
                format!("packages@{}", DOMAIN),
            )
            .await;
    }

    /// Create or get a shipment in shippo that matches this shipment.
    pub async fn create_or_get_shippo_shipment(&mut self, db: &Database) {
        // Update the formatted address.
        self.populate_formatted_address();

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
            self.oxide_tracking_link = self.oxide_tracking_link();

            // Register a tracking webhook for this shipment.
            let status = shippo_client.register_tracking_webhook(&self.carrier, &self.tracking_number).await.unwrap_or_else(|e| {
                println!("registering the tracking webhook failed: {:?}", e);
                Default::default()
            });

            let tracking_status = status.tracking_status.unwrap_or_default();
            if self.messages.is_empty() {
                self.messages = tracking_status.status_details;
            }

            // Get the status of the shipment.
            if tracking_status.status == *"TRANSIT" || tracking_status.status == "IN_TRANSIT" {
                if self.status != *"Shipped" {
                    // Send an email to the recipient with their tracking link.
                    // Wait until it is in transit to do this.
                    self.send_email_to_recipient().await;
                    // We make sure it only does this one time.
                    // Set the shipped date as this first date.
                    self.shipped_time = tracking_status.status_date;
                }

                self.status = "Shipped".to_string();
            }
            if tracking_status.status == *"DELIVERED" {
                self.status = "Delivered".to_string();
                self.delivered_time = tracking_status.status_date;
            }
            if tracking_status.status == *"RETURNED" {
                self.status = "Returned".to_string();
            }
            if tracking_status.status == *"FAILURE" {
                self.status = "Failure".to_string();
            }

            // Iterate over the tracking history and set the shipped_time.
            // Get the first date it was maked as in transit and use that as the shipped
            // time.
            for h in status.tracking_history {
                if h.status == *"TRANSIT" {
                    if let Some(shipped_time) = h.status_date {
                        let current_shipped_time = if let Some(s) = self.shipped_time { s } else { Utc::now() };

                        if shipped_time < current_shipped_time {
                            self.shipped_time = Some(shipped_time);
                        }
                    }
                }
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
            email: format!("packages@{}", DOMAIN),
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
                self.oxide_tracking_link = self.oxide_tracking_link();

                // Save it in Airtable here, in case one of the below steps fails.
                self.update(db).await;

                // Register a tracking webhook for this shipment.
                shippo_client.register_tracking_webhook(&self.carrier, &self.tracking_number).await.unwrap_or_else(|e| {
                    println!("registering the tracking webhook failed: {:?}", e);
                    Default::default()
                });

                // Print the label.
                self.print_label().await;
                self.status = "Label printed".to_string();

                // Send an email to us that we need to package the shipment.
                self.send_email_internally().await;

                break;
            }
        }

        // TODO: do something if we don't find a rate.
        // However we should always find a rate.
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

// Sync the outbound shipments.
pub async fn refresh_outbound_shipments(db: &Database) {
    // Get the GSuite token.
    let token = get_gsuite_token("").await;

    // Initialize the GSuite sheets client.
    let sheets_client = Sheets::new(token.clone());

    // Iterate over the Google sheets and get the shipments.
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

            // Parse the shipment out of the row information.
            let (shipment, sent) = NewOutboundShipment::parse_from_row_with_columns(db, &columns, &row);

            if !sent {
                let mut new_shipment = shipment.upsert(db).await;
                // Create the shipment in shippo.
                new_shipment.create_or_get_shippo_shipment(db).await;
                // Update airtable and the database again.
                new_shipment.update(db).await;
            }
        }
    }
}

// Get the sheadsheets that contain shipments.
pub fn get_shipments_spreadsheets() -> Vec<String> {
    vec!["114nnvYnUq7xuf9dw1pT90OiVpYUE6YfE_pN1wllQuCU".to_string(), "1V2NgYMlNXxxVtp81NLd_bqGllc5aDvSK2ZRqp6n2U-Y".to_string()]
}

// Sync the inbound shipments.
pub async fn refresh_inbound_shipments(db: &Database) {
    let is: Vec<airtable_api::Record<InboundShipment>> = InboundShipment::airtable().list_records(&InboundShipment::airtable_table(), "Grid view", vec![]).await.unwrap();

    for record in is {
        if record.fields.carrier.is_empty() || record.fields.tracking_number.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let mut new_shipment = NewInboundShipment {
            carrier: record.fields.carrier,
            tracking_number: record.fields.tracking_number,
            tracking_status: record.fields.tracking_status,
            name: record.fields.name,
            notes: record.fields.notes,
            delivered_time: record.fields.delivered_time,
            shipped_time: record.fields.shipped_time,
            eta: record.fields.eta,
            messages: record.fields.messages,
            oxide_tracking_link: record.fields.oxide_tracking_link,
            tracking_link: record.fields.tracking_link,
        };
        new_shipment.expand().await;
        let mut shipment = new_shipment.upsert_in_db(&db);
        if shipment.airtable_record_id.is_empty() {
            shipment.airtable_record_id = record.id;
        }
        shipment.update(&db).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::shipments::{refresh_inbound_shipments, refresh_outbound_shipments};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_shipments() {
        let db = Database::new();

        refresh_outbound_shipments(&db).await;
        refresh_inbound_shipments(&db).await;
    }
}
