#![allow(clippy::from_over_into)]
use std::convert::From;

use async_trait::async_trait;
use chrono::naive::NaiveDate;
use chrono::offset::Utc;
use chrono::{DateTime, Duration, NaiveTime};
use google_geocode::Geocode;
use macros::db;
use reqwest::StatusCode;
use schemars::JsonSchema;
use sendgrid_api::SendGrid;
use serde::{Deserialize, Serialize};
use shippo::{Address, CustomsDeclaration, CustomsItem, NewShipment, NewTransaction, Parcel, Shippo};

use crate::airtable::{AIRTABLE_INBOUND_TABLE, AIRTABLE_OUTBOUND_TABLE, AIRTABLE_PACKAGE_PICKUPS_TABLE};
use crate::companies::Company;
use crate::configs::User;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{inbound_shipments, outbound_shipments, package_pickups};

/// The data type for an inbound shipment.
#[db {
    new_struct_name = "InboundShipment",
    airtable_base = "shipments",
    airtable_table = "AIRTABLE_INBOUND_TABLE",
    match_on = {
        "carrier" = "String",
        "tracking_number" = "String",
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub order_number: String,

    /// These fields are filled in by the Airtable and should not be edited by the
    /// API updating.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
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
    airtable_base = "shipments",
    airtable_table = "AIRTABLE_OUTBOUND_TABLE",
    match_on = {
        "carrier" = "String",
        "tracking_number" = "String",
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
    #[serde(default)]
    pub latitude: f32,
    #[serde(default)]
    pub longitude: f32,
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
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
    pub label_link: String,
    #[serde(default)]
    pub cost: f32,
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
    /// Denotes the package was picked up by the user locally and we no longer
    /// need to ship it.
    #[serde(default)]
    pub local_pickup: bool,
    /// This is automatically filled in by Airtbale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_package_pickup: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

impl From<User> for NewOutboundShipment {
    fn from(user: User) -> Self {
        let db = Database::new();

        let company = Company::get_by_id(&db, user.cio_company_id);

        NewOutboundShipment {
            created_time: Utc::now(),
            name: user.full_name(),
            email: user.email(&company),
            phone: user.recovery_phone,
            street_1: user.home_address_street_1.to_string(),
            street_2: user.home_address_street_2.to_string(),
            city: user.home_address_city.to_string(),
            state: user.home_address_state.to_string(),
            zipcode: user.home_address_zipcode.to_string(),
            country: user.home_address_country.to_string(),
            address_formatted: user.home_address_formatted,
            latitude: user.home_address_latitude,
            longitude: user.home_address_longitude,
            contents: "Internal shipment: could be swag or tools, etc".to_string(),
            carrier: Default::default(),
            pickup_date: None,
            delivered_time: None,
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
            local_pickup: Default::default(),
            link_to_package_pickup: Default::default(),
            cio_company_id: user.cio_company_id,
        }
    }
}

/// The data type for a shipment pickup.
#[db {
    new_struct_name = "PackagePickup",
    airtable_base = "shipments",
    airtable_table = "AIRTABLE_PACKAGE_PICKUPS_TABLE",
    match_on = {
        "shippo_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "package_pickups"]
pub struct NewPackagePickup {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shippo_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confirmation_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transactions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_outbound_shipments: Vec<String>,
    pub requested_start_time: DateTime<Utc>,
    pub requested_end_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_start_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_end_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_by_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub messages: String,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for an PackagePickup.
#[async_trait]
impl UpdateAirtableRecord<PackagePickup> for PackagePickup {
    async fn update_airtable_record(&mut self, _record: PackagePickup) {}
}

impl OutboundShipments {
    // Always schedule the pickup for the next business day.
    // It will create a pickup for all the shipments that have "Label printed"
    // status and no pickup date currently.
    pub async fn create_pickup(db: &Database) {
        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

        // We should only do this for USPS, OR if we use DHL in the future.
        let shipments = outbound_shipments::dsl::outbound_shipments
            .filter(
                outbound_shipments::dsl::status
                    .eq("Label printed".to_string())
                    .and(outbound_shipments::dsl::carrier.eq("USPS".to_string()))
                    .and(outbound_shipments::dsl::pickup_date.is_null()),
            )
            .load::<OutboundShipment>(&db.conn())
            .unwrap();

        if shipments.is_empty() {
            // We can return early.
            return;
        }

        // Get the transaction ids, these should be the same as the shippo_id.
        let mut transaction_ids: Vec<String> = Default::default();
        let mut link_to_outbound_shipments: Vec<String> = Default::default();
        for shipment in shipments.clone() {
            println!("Adding {} shipment to our pickup", shipment.name);
            transaction_ids.push(shipment.shippo_id.to_string());
            link_to_outbound_shipments.push(shipment.airtable_record_id.to_string());
        }

        if transaction_ids.is_empty() {
            // We can return early.
            return;
        }

        // Get the carrier ID for USPS.
        // Create the shippo client.
        let shippo_client = Shippo::new_from_env();
        let carrier_accounts = shippo_client.list_carrier_accounts().await.unwrap();
        let mut carrier_account_id = "".to_string();
        for ca in carrier_accounts {
            if ca.carrier.to_lowercase() == "usps" {
                // Shippo docs say this is the object ID.
                carrier_account_id = ca.object_id;
                break;
            }
        }

        if carrier_account_id.is_empty() {
            // We can return early.
            // This should not happen.
            println!("[create_pickup] carrier account id for usps cannot be empty.");
            return;
        }

        // Get the next buisness day for pickup.
        let (start_time, end_time) = get_next_business_day();

        let pickup_date = start_time.date().naive_utc();

        let new_pickup = shippo::NewPickup {
            carrier_account: carrier_account_id.to_string(),
            location: shippo::Location {
                building_location_type: "Office".to_string(),
                building_type: "building".to_string(),
                instructions: "Knock on the glass door and someone will come open it.".to_string(),
                address: hq_address(&oxide),
            },
            transactions: transaction_ids.clone(),
            requested_start_time: start_time,
            requested_end_time: end_time,
            metadata: "".to_string(),
            is_test: false,
        };
        println!("{}", json!(new_pickup).to_string());

        let pickup = shippo_client.create_pickup(&new_pickup).await.unwrap();
        println!("{:?}", pickup);

        let mut messages = "".to_string();
        if let Some(msg) = pickup.messages {
            for m in msg {
                messages = format!("{}\n{} {} {}", messages, m.code, m.source, m.text);
            }
        }
        messages = messages.trim().to_string();

        // Let's create the new pickup in the database.
        let np = NewPackagePickup {
            shippo_id: pickup.object_id.to_string(),
            confirmation_code: pickup.confirmation_code.to_string(),
            carrier: "USPS".to_string(),
            status: pickup.status.to_string(),
            location: "Oxide HQ".to_string(),
            transactions: transaction_ids,
            link_to_outbound_shipments,
            requested_start_time: start_time,
            requested_end_time: end_time,
            confirmed_start_time: pickup.confirmed_start_time,
            confirmed_end_time: pickup.confirmed_end_time,
            cancel_by_time: pickup.cancel_by_time,
            messages,
            cio_company_id: oxide.id,
        };

        // Insert the new pickup into the database.
        np.upsert(&db).await;

        // For each of the shipments, let's set the pickup date.
        for mut shipment in shipments {
            shipment.pickup_date = Some(pickup_date);
            shipment.update(&db).await;
        }
    }
}

/// Returns the shippo data structure for the address at the office.
pub fn hq_address(company: &Company) -> Address {
    // TODO: make this the address for the company
    Address {
        company: "Oxide Computer Company".to_string(),
        name: "The Oxide Shipping Bot".to_string(),
        street1: "1251 Park Avenue".to_string(),
        city: "Emeryville".to_string(),
        state: "CA".to_string(),
        zip: "94608".to_string(),
        country: "US".to_string(),
        phone: company.phone.to_string(),
        email: format!("packages@{}", &company.gsuite_domain),
        is_complete: Default::default(),
        object_id: Default::default(),
        test: Default::default(),
        street2: Default::default(),
        validation_results: None,
    }
}

/// Returns the next buisness day in terms of start and end.
pub fn get_next_business_day() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let pacific_time = now.with_timezone(&chrono_tz::US::Pacific);

    let mut next_day = pacific_time.checked_add_signed(Duration::days(1)).unwrap();
    let day_of_week_string = next_day.format("%A").to_string();
    if day_of_week_string == "Saturday" {
        next_day = pacific_time.checked_add_signed(Duration::days(3)).unwrap();
    } else if day_of_week_string == "Sunday" {
        next_day = pacific_time.checked_add_signed(Duration::days(2)).unwrap();
    }

    // Let's create the start time, which should be around 9am.
    let start_time = next_day.date().and_time(NaiveTime::from_hms(8, 59, 59)).unwrap();

    // Let's create the end time, which should be around 5pm.
    let end_time = next_day.date().and_time(NaiveTime::from_hms(16, 59, 59)).unwrap();

    (start_time.with_timezone(&Utc), end_time.with_timezone(&Utc))
}

/// Implement updating the Airtable record for an OutboundShipment.
#[async_trait]
impl UpdateAirtableRecord<OutboundShipment> for OutboundShipment {
    async fn update_airtable_record(&mut self, record: OutboundShipment) {
        self.link_to_package_pickup = record.link_to_package_pickup;

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
    pub async fn print_label(&self, db: &Database) {
        if self.label_link.trim().is_empty() {
            // Return early.
            return;
        }

        let company = Company::get_by_id(db, self.cio_company_id);

        if company.printer_url.is_empty() {
            // Return early.
            return;
        }

        let printer_url = format!("{}/rollo", company.printer_url);
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

    /// Send an email to the recipient with their order information.
    /// This should happen before they get the email that it has been shipped.
    pub async fn send_email_to_recipient_pre_shipping(&self, company: &Company) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .send_mail(
                format!("{}, your order from the Oxide Computer Company has been received!", self.name),
                format!(
                    "Below is the information for your order:

**Contents:**
{}

**Address to:**
{}
{}

You will receive another email once your order has been shipped with your tracking numbers.

If you have any questions or concerns, please respond to this email!
Have a splendid day!

xoxo,
  The Oxide Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                ),
                vec![self.email.to_string()],
                vec![format!("packages@{}", &company.gsuite_domain)],
                vec![],
                format!("packages@{}", &company.gsuite_domain),
            )
            .await;
    }

    /// Send an email to the recipient with their tracking code and information.
    pub async fn send_email_to_recipient(&self, company: &Company) {
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .send_mail(
                format!("{}, your package from the Oxide Computer Company is on the way!", self.name),
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
                vec![format!("packages@{}", &company.gsuite_domain)],
                vec![],
                format!("packages@{}", &company.gsuite_domain),
            )
            .await;
    }

    /// Send an email internally that we need to package the shipment.
    pub async fn send_email_internally(&self, company: &Company) {
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

The label should already be printed on the cart with the label printers. Please
take the label and affix it to the package with the specified contents. It can
then be dropped off for {}.

You DO NOT need to scan the barcodes of the items since they have already been
deducted from inventory. DO NOT SCAN THE BARCODES for the items since
they have already been deducted from inventory.

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
                vec![format!("packages@{}", &company.gsuite_domain)],
                vec![],
                vec![],
                format!("packages@{}", &company.gsuite_domain),
            )
            .await;
    }

    /// Create or get a shipment in shippo that matches this shipment.
    pub async fn create_or_get_shippo_shipment(&mut self, db: &Database) {
        let company = Company::get_by_id(db, self.cio_company_id);

        // Update the formatted address.
        self.populate_formatted_address();

        // Create the shippo client.
        let shippo_client = Shippo::new_from_env();
        // Create the geocode client.
        let geocode = Geocode::new_from_env();

        // If we don't already have the latitude and longitude for the shipment
        // let's update that first.
        if self.latitude == 0.0 || self.longitude == 0.0 {
            let result = geocode.get(&clean_address_string(&self.address_formatted)).await.unwrap();
            let location = result.geometry.location;
            self.latitude = location.lat as f32;
            self.longitude = location.lng as f32;
            // Update here just in case something goes wrong later.
            self.update(db).await;
        }

        // If we did local_pickup, we can return early here.
        if self.local_pickup {
            self.status = "Picked up".to_string();
            self.update(db).await;
            // Return early.
            return;
        }

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
                let mut messages = "".to_string();
                for m in label.messages {
                    messages = format!("{}\n{} {} {}", messages, m.code, m.source, m.text);
                }
                self.messages = messages.trim().to_string();
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
                    self.send_email_to_recipient(&company).await;
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
        let address_from = hq_address(&company);

        // If this is an international shipment, we need to define our customs
        // declarations.
        let mut cd: Option<CustomsDeclaration> = None;
        if self.country != "US" {
            let mut cd_inner: CustomsDeclaration = Default::default();
            // Create customs items for each item in our order.
            for line in self.contents.lines() {
                let mut ci: CustomsItem = Default::default();
                ci.description = line.to_string();
                let (prefix, _suffix) = line.split_once(" x ").unwrap_or(("1", ""));
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

        if self.country == "Great Britain" {
            self.country = "GB".to_string();
        } else if self.country == "United States" {
            self.country = "US".to_string();
        }

        // We need a phone number for the shipment.
        if self.phone.is_empty() {
            // Use the Oxide office line.
            self.phone = company.phone.to_string();
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
                    metadata: "Default box for swag".to_string(),
                    length: "12".to_string(),
                    width: "12".to_string(),
                    height: "6".to_string(),
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
                    let mut messages = "".to_string();
                    for m in label.messages {
                        messages = format!("{}\n{} {} {}", messages, m.code, m.source, m.text);
                    }
                    self.messages = messages.trim().to_string();
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
                self.print_label(db).await;
                self.status = "Label printed".to_string();

                // Send an email to us that we need to package the shipment.
                self.send_email_internally(&company).await;

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
    // Iterate over all the shipments in the database and update them.
    // This ensures that any one offs (that don't come from spreadsheets) are also updated.
    // TODO: if we decide to accept one-offs straight in airtable support that, but for now
    // we do not.
    let shipments = OutboundShipments::get_from_db(&db);
    for mut s in shipments {
        if let Some(existing) = s.get_existing_airtable_record(&db).await {
            // Take the field from Airtable.
            s.local_pickup = existing.fields.local_pickup;
        }

        // Update the shipment from shippo.
        s.create_or_get_shippo_shipment(db).await;
        // Update airtable and the database again.
        s.update(db).await;
    }
}

// Sync the inbound shipments.
pub async fn refresh_inbound_shipments(db: &Database) {
    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(db, "Oxide".to_string()).unwrap();

    let is: Vec<airtable_api::Record<InboundShipment>> = oxide
        .authenticate_airtable(&oxide.airtable_base_id_shipments)
        .list_records(&InboundShipment::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();

    for record in is {
        if record.fields.carrier.is_empty() || record.fields.tracking_number.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let mut new_shipment: NewInboundShipment = record.fields.into();
        new_shipment.expand().await;
        new_shipment.cio_company_id = oxide.id;
        let mut shipment = new_shipment.upsert_in_db(&db);
        if shipment.airtable_record_id.is_empty() {
            shipment.airtable_record_id = record.id;
        }
        shipment.update(&db).await;
    }

    InboundShipments::get_from_db(&db).update_airtable(&db, oxide.id).await;
}

pub fn clean_address_string(s: &str) -> String {
    if s == "DE" {
        return "Germany".to_string();
    } else if s == "GB" {
        return "Great Britian".to_string();
    }

    s.to_string()
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::shipments::{refresh_inbound_shipments, refresh_outbound_shipments, OutboundShipments};

    /*#[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_pickup() {
        let db = Database::new();

        OutboundShipments::create_pickup(&db).await;
    }*/

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_shipments() {
        let db = Database::new();

        refresh_inbound_shipments(&db).await;
        refresh_outbound_shipments(&db).await;
    }
}
