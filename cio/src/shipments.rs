#![allow(clippy::from_over_into)]
use std::convert::From;

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{naive::NaiveDate, offset::Utc, DateTime, Duration, NaiveTime};
use chrono_humanize::HumanTime;
use google_geocode::Geocode;
use log::{info, warn};
use macros::db;
use reqwest::StatusCode;
use schemars::JsonSchema;
use sendgrid_api::{traits::MailOps, Client as SendGrid};
use serde::{Deserialize, Serialize};
use shippo::{Address, CustomsDeclaration, CustomsItem, NewShipment, NewTransaction, Parcel, Shippo};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockText, MessageBlockType, MessageType,
};

use crate::{
    airtable::{AIRTABLE_INBOUND_TABLE, AIRTABLE_OUTBOUND_TABLE, AIRTABLE_PACKAGE_PICKUPS_TABLE},
    companies::Company,
    configs::User,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{inbound_shipments, outbound_shipments, package_pickups},
};

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
#[diesel(table_name = inbound_shipments)]
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
    async fn update_airtable_record(&mut self, record: InboundShipment) -> Result<()> {
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

        Ok(())
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
            self.tracking_link = format!(
                "https://tools.usps.com/go/TrackConfirmAction_input?origTrackNum={}",
                self.tracking_number
            );
        } else if carrier == "ups" {
            self.tracking_link = format!("https://www.ups.com/track?tracknum={}", self.tracking_number);
        } else if carrier == "fedex" {
            self.tracking_link = format!(
                "https://www.fedex.com/apps/fedextrack/?tracknumbers={}",
                self.tracking_number
            );
        } else if carrier == "dhl" {
            // TODO: not sure if this one is correct.
            self.tracking_link = format!(
                "https://www.dhl.com/en/express/tracking.html?AWB={}",
                self.tracking_number
            );
        }
    }

    /// Get the details about the shipment from the tracking API.
    pub async fn expand(&mut self) -> Result<()> {
        // Create the shippo client.
        let shippo = Shippo::new_from_env();

        let mut carrier = self.carrier.to_lowercase().to_string();
        if carrier == "dhl" {
            carrier = "dhl_express".to_string();
        }

        // Get the tracking status for the shipment and fill in the details.
        let ts = shippo.get_tracking_status(&carrier, &self.tracking_number).await?;
        self.tracking_number = ts.tracking_number.to_string();
        let mut status = ts.tracking_status.unwrap_or_default();
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
            } else if h.status == *"DELIVERED" {
                status.status = "DELIVERED".to_string();
                if h.status_date.is_some() {
                    self.delivered_time = h.status_date;
                }
            }
        }

        if status.status == *"DELIVERED" && status.status_date.is_some() {
            self.delivered_time = status.status_date;
        }

        if self.delivered_time.is_some() {
            status.status = "DELIVERED".to_string();
        }

        // Register a tracking webhook for this shipment.
        shippo
            .register_tracking_webhook(&carrier, &self.tracking_number)
            .await?;

        // Set the new status.
        self.tracking_status = status.status.to_string();

        Ok(())
    }
}

impl InboundShipment {
    /// Get the details about the shipment from the tracking API.
    pub async fn expand(&mut self, db: &Database) -> Result<()> {
        let mut ns: NewInboundShipment = self.clone().into();
        ns.expand().await?;
        ns.upsert(db).await?;
        Ok(())
    }
}

fn get_color_based_on_tracking_status(s: &str) -> String {
    let status = s.to_lowercase().trim().to_string();

    if status == "delivered" {
        return crate::colors::Colors::Green.to_string();
    }
    if status == "transit" {
        return crate::colors::Colors::Blue.to_string();
    }
    if status == "pre_transit" {
        return crate::colors::Colors::Yellow.to_string();
    }
    if status == "returned" || status == "unknown" {
        return crate::colors::Colors::Red.to_string();
    }

    // Otherwise return yellow as default if we don't know.
    crate::colors::Colors::Yellow.to_string()
}

/// Convert the inbound shipment into a Slack message.
impl From<NewInboundShipment> for FormattedMessage {
    fn from(item: NewInboundShipment) -> Self {
        let mut status_msg = format!(
            "Inbound shipment | *{}* | <{}|{}>",
            item.tracking_status,
            item.oxide_tracking_link,
            item.oxide_tracking_link.trim_start_matches("https://"),
        );
        if let Some(eta) = item.eta {
            if item.tracking_status != "DELIVERED" {
                let dur = eta - Utc::now();

                status_msg += &format!(" | _eta {}_", HumanTime::from(dur));
            }
        }
        if item.tracking_status != "DELIVERED" {
            if let Some(delivered) = item.delivered_time {
                let dur = delivered - Utc::now();

                status_msg += &format!(" | _delivered {}_", HumanTime::from(dur));
            }
        }

        let mut notes = String::new();
        if !item.order_number.is_empty() {
            notes = format!("order #: {}", item.order_number);
        } else if !item.notes.starts_with("Parsed email from") && !item.notes.is_empty() {
            notes = item.notes.to_string();
        }

        let mut blocks = vec![MessageBlock {
            block_type: MessageBlockType::Header,
            text: Some(MessageBlockText {
                text_type: MessageType::PlainText,
                text: item.name,
            }),
            elements: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        }];

        if !notes.is_empty() {
            blocks.push(MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: notes,
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            });
        }
        blocks.push(MessageBlock {
            block_type: MessageBlockType::Context,
            elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                text_type: MessageType::Markdown,
                text: status_msg,
            })],
            text: Default::default(),
            accessory: Default::default(),
            block_id: Default::default(),
            fields: Default::default(),
        });

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: get_color_based_on_tracking_status(&item.tracking_status),
                author_icon: Default::default(),
                author_link: Default::default(),
                author_name: Default::default(),
                fallback: Default::default(),
                fields: Default::default(),
                footer: Default::default(),
                footer_icon: Default::default(),
                image_url: Default::default(),
                pretext: Default::default(),
                text: Default::default(),
                thumb_url: Default::default(),
                title: Default::default(),
                title_link: Default::default(),
                ts: Default::default(),
                blocks,
            }],
        }
    }
}

impl From<InboundShipment> for FormattedMessage {
    fn from(item: InboundShipment) -> Self {
        let new: NewInboundShipment = item.into();
        new.into()
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
            self.tracking_link = format!(
                "https://tools.usps.com/go/TrackConfirmAction_input?origTrackNum={}",
                self.tracking_number
            );
        } else if carrier == "ups" {
            self.tracking_link = format!("https://www.ups.com/track?tracknum={}", self.tracking_number);
        } else if carrier == "fedex" {
            self.tracking_link = format!(
                "https://www.fedex.com/apps/fedextrack/?tracknumbers={}",
                self.tracking_number
            );
        } else if carrier == "dhl" {
            // TODO: not sure if this one is correct.
            self.tracking_link = format!(
                "https://www.dhl.com/en/express/tracking.html?AWB={}",
                self.tracking_number
            );
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
#[diesel(table_name = outbound_shipments)]
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
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
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
    pub provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_id: String,
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
        NewOutboundShipment {
            created_time: Utc::now(),
            name: user.full_name(),
            email: user.email,
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
            provider: "Shippo".to_string(),
            provider_id: Default::default(),
            status: crate::shipment_status::Status::Queued.to_string(),
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

impl From<shipbob::types::Order> for NewOutboundShipment {
    fn from(item: shipbob::types::Order) -> Self {
        let recipient = item.recipient.unwrap();

        let mut contents = String::new();

        let mut carrier = String::new();
        let mut tracking_number = String::new();
        let mut shipped_time = None;
        let mut status = crate::shipment_status::Status::Queued;
        let mut tracking_status = String::new();
        let mut tracking_link = String::new();
        let mut cost = Default::default();

        if let Some(s) = item.status {
            status = s.into();
        }

        if !item.shipments.is_empty() {
            let first = item.shipments.first().unwrap();
            shipped_time = first.created_date;
            cost = first.invoice_amount as f32;

            if let Some(tracking) = &first.tracking {
                carrier = clean_carrier_name(&tracking.carrier);
                tracking_number = tracking.tracking_number.to_string();
                tracking_link = tracking.tracking_url.to_string();
            }

            if let Some(s) = &first.status {
                tracking_status = s.to_string();
            }

            for p in &first.products {
                for i in &p.inventory_items {
                    contents += &format!("\n{} x {}", i.quantity, i.name);
                }
            }
        }

        contents = contents.trim().to_string();

        NewOutboundShipment {
            provider: "ShipBob".to_string(),
            provider_id: item.id.to_string(),
            created_time: item.created_date.unwrap(),
            name: recipient.name.to_string(),
            email: recipient.email.to_string(),
            phone: recipient.phone_number.to_string(),
            street_1: recipient.address.address_1.to_string(),
            street_2: recipient.address.address_2.to_string(),
            city: recipient.address.city.to_string(),
            state: recipient.address.state.to_string(),
            zipcode: recipient.address.zip_code.to_string(),
            country: recipient.address.country,

            contents,
            carrier,
            tracking_number,
            tracking_status,
            delivered_time: None,
            shipped_time,
            tracking_link,
            status: status.to_string(),
            cost,
            eta: None,

            // These will be poulated when we expand the record.
            address_formatted: Default::default(),
            latitude: Default::default(),
            longitude: Default::default(),
            oxide_tracking_link: Default::default(),

            // These don't apply.
            pickup_date: None,
            label_link: Default::default(),
            messages: Default::default(),
            notes: Default::default(),
            geocode_cache: Default::default(),
            local_pickup: Default::default(),
            link_to_package_pickup: Default::default(),
            cio_company_id: Default::default(),
        }
    }
}

impl From<shipbob::types::Status> for crate::shipment_status::Status {
    fn from(item: shipbob::types::Status) -> Self {
        match item {
            shipbob::types::Status::Cancelled => crate::shipment_status::Status::Cancelled,
            shipbob::types::Status::CleanSweeped => crate::shipment_status::Status::CleanSweeped,
            shipbob::types::Status::Completed => crate::shipment_status::Status::Delivered,
            shipbob::types::Status::Exception => crate::shipment_status::Status::Failure,
            shipbob::types::Status::ImportReview => crate::shipment_status::Status::ImportReview,
            shipbob::types::Status::LabeledCreated => crate::shipment_status::Status::LabelCreated,
            shipbob::types::Status::None => crate::shipment_status::Status::None,
            shipbob::types::Status::OnHold => crate::shipment_status::Status::OnHold,
            shipbob::types::Status::Pending => crate::shipment_status::Status::Queued,
            shipbob::types::Status::Processing => crate::shipment_status::Status::Processing,
            shipbob::types::Status::Noop => crate::shipment_status::Status::Queued,
            shipbob::types::Status::FallthroughString => crate::shipment_status::Status::Queued,
        }
    }
}

impl From<shipbob::types::OrderStatus> for crate::shipment_status::Status {
    fn from(item: shipbob::types::OrderStatus) -> Self {
        match item {
            shipbob::types::OrderStatus::Cancelled => crate::shipment_status::Status::Cancelled,
            shipbob::types::OrderStatus::Fulfilled => crate::shipment_status::Status::Delivered,
            shipbob::types::OrderStatus::Exception => crate::shipment_status::Status::Failure,
            shipbob::types::OrderStatus::ImportReview => crate::shipment_status::Status::ImportReview,
            shipbob::types::OrderStatus::PartiallyFulfilled => crate::shipment_status::Status::PartiallyFulfilled,
            shipbob::types::OrderStatus::Processing => crate::shipment_status::Status::Processing,
            shipbob::types::OrderStatus::Noop => crate::shipment_status::Status::Queued,
            shipbob::types::OrderStatus::FallthroughString => crate::shipment_status::Status::Queued,
        }
    }
}

/// Convert the outbound shipment into a Slack message.
impl From<NewOutboundShipment> for FormattedMessage {
    fn from(item: NewOutboundShipment) -> Self {
        let mut status_msg = format!(
            "Outbound shipment | *{}* | _{}_ | <{}|{}>",
            item.tracking_status,
            item.status,
            item.oxide_tracking_link,
            item.oxide_tracking_link.trim_start_matches("https://"),
        );
        if let Some(eta) = item.eta {
            if item.status != crate::shipment_status::Status::Delivered.to_string()
                && item.status != crate::shipment_status::Status::PickedUp.to_string()
            {
                let dur = eta - Utc::now();

                status_msg += &format!(" | _eta {}_", HumanTime::from(dur));
            }
        }

        if item.status == crate::shipment_status::Status::Delivered.to_string() {
            if let Some(delivered) = item.delivered_time {
                let dur = delivered - Utc::now();

                status_msg += &format!(" | _delivered {}_", HumanTime::from(dur));
            }
        }

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: get_color_based_on_tracking_status(&item.tracking_status),
                author_icon: Default::default(),
                author_link: Default::default(),
                author_name: Default::default(),
                fallback: Default::default(),
                fields: Default::default(),
                footer: Default::default(),
                footer_icon: Default::default(),
                image_url: Default::default(),
                pretext: Default::default(),
                text: Default::default(),
                thumb_url: Default::default(),
                title: Default::default(),
                title_link: Default::default(),
                ts: Default::default(),
                blocks: vec![
                    MessageBlock {
                        block_type: MessageBlockType::Header,
                        text: Some(MessageBlockText {
                            text_type: MessageType::PlainText,
                            text: item.name.to_string(),
                        }),
                        elements: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Section,
                        text: Some(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: item.contents,
                        }),
                        elements: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                    MessageBlock {
                        block_type: MessageBlockType::Context,
                        elements: vec![slack_chat_api::BlockOption::MessageBlockText(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text: status_msg,
                        })],
                        text: Default::default(),
                        accessory: Default::default(),
                        block_id: Default::default(),
                        fields: Default::default(),
                    },
                ],
            }],
        }
    }
}

impl From<OutboundShipment> for FormattedMessage {
    fn from(item: OutboundShipment) -> Self {
        let new: NewOutboundShipment = item.into();
        new.into()
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
#[diesel(table_name = package_pickups)]
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
    async fn update_airtable_record(&mut self, _record: PackagePickup) -> Result<()> {
        Ok(())
    }
}

impl OutboundShipments {
    // Always schedule the pickup for the next business day.
    // It will create a pickup for all the shipments that have "Label printed"
    // status and no pickup date currently.
    pub async fn create_pickup(db: &Database, company: &Company) -> Result<()> {
        // We should only do this for USPS, OR if we use DHL in the future.
        let shipments = outbound_shipments::dsl::outbound_shipments
            .filter(
                outbound_shipments::dsl::status
                    .eq(crate::shipment_status::Status::LabelPrinted.to_string())
                    .and(outbound_shipments::dsl::carrier.eq("USPS".to_string()))
                    .and(outbound_shipments::dsl::provider.eq("Shippo".to_string()))
                    .and(outbound_shipments::dsl::pickup_date.is_null()),
            )
            .load_async::<OutboundShipment>(db.pool())
            .await?;

        if shipments.is_empty() {
            // We can return early.
            return Ok(());
        }

        // Get the transaction ids, these should be the same as the provider_id.
        let mut transaction_ids: Vec<String> = Default::default();
        let mut link_to_outbound_shipments: Vec<String> = Default::default();
        for shipment in shipments.clone() {
            info!("adding {} shipment to our pickup", shipment.name);
            transaction_ids.push(shipment.provider_id.to_string());
            link_to_outbound_shipments.push(shipment.airtable_record_id.to_string());
        }

        if transaction_ids.is_empty() {
            // We can return early.
            return Ok(());
        }

        // Get the carrier ID for USPS.
        // Create the shippo client.
        let shippo_client = Shippo::new_from_env();
        let carrier_accounts = shippo_client.list_carrier_accounts().await?;
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
            warn!("create pickup carrier account id for usps cannot be empty.");
            return Ok(());
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
                address: company.hq_shipping_address(db).await?,
            },
            transactions: transaction_ids.clone(),
            requested_start_time: start_time,
            requested_end_time: end_time,
            metadata: "".to_string(),
            is_test: false,
        };

        let pickup = shippo_client.create_pickup(&new_pickup).await?;

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
            location: "HQ".to_string(),
            transactions: transaction_ids,
            link_to_outbound_shipments,
            requested_start_time: start_time,
            requested_end_time: end_time,
            confirmed_start_time: pickup.confirmed_start_time,
            confirmed_end_time: pickup.confirmed_end_time,
            cancel_by_time: pickup.cancel_by_time,
            messages,
            cio_company_id: company.id,
        };

        // Insert the new pickup into the database.
        np.upsert(db).await?;

        // For each of the shipments, let's set the pickup date.
        for mut shipment in shipments {
            shipment.pickup_date = Some(pickup_date);
            shipment
                .set_status(crate::shipment_status::Status::WaitingForPickup)
                .await?;
            shipment.update(db).await?;
        }

        Ok(())
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
    async fn update_airtable_record(&mut self, record: OutboundShipment) -> Result<()> {
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
        if self.provider_id.is_empty() {
            self.provider_id = record.provider_id;
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

        Ok(())
    }
}

impl OutboundShipment {
    fn populate_formatted_address(&mut self) {
        let mut street_address = self.street_1.to_string();
        if !self.street_2.is_empty() {
            street_address = format!("{}\n{}", self.street_1, self.street_2,);
        }
        self.address_formatted = format!(
            "{}\n{}, {} {} {}",
            street_address, self.city, self.state, self.zipcode, self.country
        )
        .trim()
        .trim_matches(',')
        .trim()
        .to_string();
    }

    pub fn oxide_tracking_link(&self) -> String {
        format!("https://track.oxide.computer/{}/{}", self.carrier, self.tracking_number)
    }

    /// Send the receipt to our printer.
    pub async fn print_receipt(&self, db: &Database) -> Result<()> {
        if self.contents.trim().is_empty() {
            // Return early.
            return Ok(());
        }

        let company = self.company(db).await?;

        if company.printer_url.is_empty() {
            // Return early.
            return Ok(());
        }

        let printer_url = format!("{}/receipt", company.printer_url);
        let client = reqwest::Client::new();
        let resp = client
            .post(&printer_url)
            .body(
                json!(crate::swag_inventory::PrintRequest {
                    content: format!(
                        "{}\n{}\n\n{}\n{}\n\n{}\n\n",
                        self.name, self.address_formatted, self.carrier, self.tracking_number, self.contents
                    ),
                    quantity: 1,
                    url: String::new(),
                })
                .to_string(),
            )
            .send()
            .await?;
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => {
                bail!("[print]: status_code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Send the label to our printer.
    pub async fn print_label(&self, db: &Database) -> Result<()> {
        if self.label_link.trim().is_empty() {
            // Return early.
            return Ok(());
        }

        let company = self.company(db).await?;

        if company.printer_url.is_empty() {
            // Return early.
            return Ok(());
        }

        let printer_url = format!("{}/rollo", company.printer_url);
        let client = reqwest::Client::new();
        let resp = client
            .post(&printer_url)
            .body(json!(self.label_link).to_string())
            .send()
            .await?;
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => {
                bail!("[print]: status_code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }

    /// Format address.
    pub fn format_address(&self) -> String {
        let mut street = self.street_1.to_string();
        if !self.street_2.is_empty() {
            street = format!("{}\n{}", self.street_1, self.street_2);
        }

        format!(
            "{}\n{}, {} {} {}",
            street, self.city, self.state, self.zipcode, self.country
        )
    }

    /// Send an email to the recipient with their order information.
    /// This should happen before they get the email that it has been shipped.
    pub async fn send_email_to_recipient_pre_shipping(&self, db: &Database) -> Result<()> {
        if self.email.is_empty() {
            return Ok(());
        }

        let company = self.company(db).await?;

        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &format!("{}, your order from {} has been received!", self.name, company.name),
                &format!(
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
  The Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                ),
                &[self.email.to_string()],
                &[format!("packages@{}", &company.gsuite_domain)],
                &[],
                &format!("packages@{}", &company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Send an email to the recipient with their tracking code and information.
    pub async fn send_email_to_recipient(&self, db: &Database) -> Result<()> {
        if self.email.is_empty() {
            return Ok(());
        }

        let company = self.company(db).await?;
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &format!("{}, your package from {} is on the way!", self.name, company.name),
                &format!(
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
  The Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                    self.oxide_tracking_link
                ),
                &[self.email.to_string()],
                &[format!("packages@{}", &company.gsuite_domain)],
                &[],
                &format!("packages@{}", &company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Send an email internally that we need to package the shipment.
    pub async fn send_email_internally(&self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;
        // Initialize the SendGrid client.
        let sendgrid_client = SendGrid::new_from_env();
        // Send the message.
        sendgrid_client
            .mail_send()
            .send_plain_text(
                &format!("Shipment to {} is ready to be packaged", self.name),
                &format!(
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

The Shipping Bot",
                    self.contents,
                    self.name,
                    self.format_address(),
                    self.oxide_tracking_link,
                    self.carrier,
                ),
                &[format!("packages@{}", &company.gsuite_domain)],
                &[],
                &[],
                &format!("packages@{}", &company.gsuite_domain),
            )
            .await?;

        Ok(())
    }

    /// Sends a Slack notification if the status of the shipment changed.
    /// And changes the status of the shipment.
    pub async fn set_status(&mut self, status: crate::shipment_status::Status) -> Result<()> {
        // Set the new status.
        self.status = status.to_string();

        Ok(())
    }

    pub async fn set_lat_lng(&mut self, db: &Database) -> Result<()> {
        // Create the geocode client.
        let geocode = Geocode::new_from_env();

        // If we don't already have the latitude and longitude for the shipment
        // let's update that first.
        if self.latitude == 0.0 || self.longitude == 0.0 {
            let result = geocode.get(&clean_address_string(&self.address_formatted)).await?;
            let location = result.geometry.location;
            self.latitude = location.lat as f32;
            self.longitude = location.lng as f32;
            // Update here just in case something goes wrong later.
            self.update(db).await?;
        }

        Ok(())
    }

    pub async fn expand(&mut self, db: &Database) -> Result<()> {
        // Update the formatted address.
        self.populate_formatted_address();

        // Update the lat and lng.
        self.set_lat_lng(db).await?;

        // Update the tracking status.
        // Create the shippo client.
        let shippo = Shippo::new_from_env();

        let mut carrier = self.carrier.to_lowercase().to_string();
        if carrier == "dhl" {
            carrier = "dhl_express".to_string();
        }

        if carrier.is_empty() || self.tracking_number.is_empty() {
            return Ok(());
        }

        // Get the tracking status for the shipment and fill in the details.
        let ts = shippo.get_tracking_status(&carrier, &self.tracking_number).await?;
        self.tracking_number = ts.tracking_number.to_string();
        let mut status = ts.tracking_status.unwrap_or_default();
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
            } else if h.status == *"DELIVERED" {
                status.status = "DELIVERED".to_string();
                if h.status_date.is_some() {
                    self.delivered_time = h.status_date;
                }
            }
        }

        if status.status == *"DELIVERED" && status.status_date.is_some() {
            self.delivered_time = status.status_date;
        }

        if self.delivered_time.is_some() {
            status.status = "DELIVERED".to_string();
        }

        // Register a tracking webhook for this shipment.
        shippo
            .register_tracking_webhook(&carrier, &self.tracking_number)
            .await?;

        // Set the new status.
        self.tracking_status = status.status.to_string();

        // Update in the database.
        self.update(db).await?;

        Ok(())
    }

    /// Create or get a shipment in shippo that matches this shipment.
    pub async fn create_or_get_shippo_shipment(&mut self, db: &Database) -> Result<()> {
        if self.provider != "Shippo" {
            // Return early it's not a shippo shipment.
            return Ok(());
        }

        let company = self.company(db).await?;

        // Update the formatted address.
        self.populate_formatted_address();

        // Update the lat and lng.
        self.set_lat_lng(db).await?;

        // Create the shippo client.
        let shippo_client = Shippo::new_from_env();

        // If we did local_pickup, we can return early here.
        if self.local_pickup {
            self.set_status(crate::shipment_status::Status::PickedUp).await?;
            self.update(db).await?;
            // Return early.
            return Ok(());
        }

        // If we already have a shippo id, get the information for the label.
        if !self.provider_id.is_empty() {
            let label = shippo_client.get_shipping_label(&self.provider_id).await?;

            // Set the additional fields.
            self.tracking_number = label.tracking_number;
            self.tracking_link = label.tracking_url_provider;
            self.tracking_status = label.tracking_status;
            self.label_link = label.label_url;
            self.eta = label.eta;
            self.provider_id = label.object_id;
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
            let status = shippo_client
                .register_tracking_webhook(&self.carrier, &self.tracking_number)
                .await?;

            let tracking_status = status.tracking_status.unwrap_or_default();
            if self.messages.is_empty() {
                self.messages = tracking_status.status_details;
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

            // Get the status of the shipment.
            if tracking_status.status == *"TRANSIT" || tracking_status.status == "IN_TRANSIT" {
                if self.status != crate::shipment_status::Status::Shipped.to_string() {
                    // Send an email to the recipient with their tracking link.
                    // Wait until it is in transit to do this.
                    self.send_email_to_recipient(db).await?;
                    // We make sure it only does this one time.
                    // Set the shipped date as this first date.
                    self.shipped_time = tracking_status.status_date;
                }

                self.set_status(crate::shipment_status::Status::Shipped).await?;
            }
            if tracking_status.status == *"DELIVERED" {
                self.delivered_time = tracking_status.status_date;
                self.set_status(crate::shipment_status::Status::Delivered).await?;
            }
            if tracking_status.status == *"RETURNED" {
                self.set_status(crate::shipment_status::Status::Returned).await?;
            }
            if tracking_status.status == *"FAILURE" {
                self.set_status(crate::shipment_status::Status::Failure).await?;
            }

            // Return early.
            return Ok(());
        }

        // We need to create the label since we don't have one already.
        let address_from = company.hq_shipping_address(db).await?;

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
                ci.quantity = prefix.parse()?;
                ci.net_weight = "0.25".to_string();
                ci.mass_unit = "lb".to_string();
                ci.value_amount = "100.00".to_string();
                ci.value_currency = "USD".to_string();
                ci.origin_country = "US".to_string();
                let c = shippo_client.create_customs_item(ci).await?;

                // Add the item to our array of items.
                cd_inner.items.push(c.object_id);
            }

            // Fill out the rest of the customs declaration fields.
            // TODO: make this modifiable.
            cd_inner.certify_signer = "Jess Frazelle".to_string();
            cd_inner.certify = true;
            cd_inner.non_delivery_option = "RETURN".to_string();
            cd_inner.contents_type = "GIFT".to_string();
            // This can only have a max of 200 chars.
            // Weird I know.
            cd_inner.contents_explanation = crate::utils::truncate(&self.contents, 200);
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
            // Use the company phone line.
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
                    weight: "2".to_string(),
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
            .await?;

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
                    .await?;

                // Set the additional fields.
                self.carrier = clean_carrier_name(&rate.provider);
                self.cost = rate.amount_local.parse()?;
                self.tracking_number = label.tracking_number.to_string();
                self.tracking_link = label.tracking_url_provider.to_string();
                self.tracking_status = label.tracking_status.to_string();
                self.label_link = label.label_url.to_string();
                self.eta = label.eta;
                self.provider_id = label.object_id.to_string();
                self.oxide_tracking_link = self.oxide_tracking_link();
                if label.status != "SUCCESS" {
                    // Print the messages in the messages field.
                    let mut messages = "".to_string();
                    for m in label.messages {
                        messages = format!("{}\n{} {} {}", messages, m.code, m.source, m.text);
                    }
                    self.messages = messages.trim().to_string();
                } else {
                    self.set_status(crate::shipment_status::Status::LabelCreated).await?;
                }

                // Save it in Airtable here, in case one of the below steps fails.
                self.update(db).await?;

                // Register a tracking webhook for this shipment.
                shippo_client
                    .register_tracking_webhook(&self.carrier, &self.tracking_number)
                    .await?;

                // Print the label.
                self.print_label(db).await?;
                // Print the receipt.
                self.print_receipt(db).await?;
                self.set_status(crate::shipment_status::Status::LabelPrinted).await?;

                // Send an email to us that we need to package the shipment.
                self.send_email_internally(db).await?;

                break;
            }
        }

        // TODO: do something if we don't find a rate.
        // However we should always find a rate.
        Ok(())
    }
}

// Sync the outbound shipments.
pub async fn refresh_outbound_shipments(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_shipments.is_empty() {
        // Return early.
        return Ok(());
    }

    // Let's see if we can get any shipments from ShipBob.
    if let Ok(shipbob) = company.authenticate_shipbob().await {
        match shipbob
            .orders()
            .get_all(
                &[],  // ids
                &[],  // reference_ids
                None, // start_date
                None, // end_date
                shipbob::types::SortOrder::Newest,
                false, // has_tracking
                None,  // last_update_start_date
                None,  // last_update_end_date
                false, // is_tracking_uploaded
            )
            .await
        {
            Ok(orders) => {
                // Iterate over the orders and add them as a shipment.
                for o in orders {
                    let mut ns: NewOutboundShipment = o.into();
                    // Be sure to set the company id.
                    ns.cio_company_id = company.id;

                    // Update the database.
                    let mut s = ns.upsert(db).await?;

                    // Expand the shipment.
                    // This will also update the database.
                    s.expand(db).await?;
                }
            }
            Err(e) => {
                warn!("getting shipbob orders failed: {}", e);
            }
        }
    }

    // Iterate over all the shipments in the database and update them.
    // This ensures that any one offs (that don't come from spreadsheets) are also updated.
    // TODO: if we decide to accept one-offs straight in airtable support that, but for now
    // we do not.
    let shipments = OutboundShipments::get_from_db(db, company.id).await?;
    for mut s in shipments {
        if let Some(existing) = s.get_existing_airtable_record(db).await {
            // Take the field from Airtable.
            s.local_pickup = existing.fields.local_pickup;
        }

        // Update the shipment from shippo, this will only apply if the provider is set as "Shippo".
        s.create_or_get_shippo_shipment(db).await?;

        // Update airtable and the database again.
        s.update(db).await?;
    }

    update_manual_shippo_shipments(db, company).await?;

    OutboundShipments::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;
    Ok(())
}

pub fn clean_carrier_name(s: &str) -> String {
    let l = s.to_lowercase();
    if l == "ups" || l.starts_with("ups") {
        return "UPS".to_string();
    } else if l == "fedex" {
        return "FedEx".to_string();
    } else if l == "usps" {
        return "USPS".to_string();
    } else if l == "dhl" || l == "dhl_express" || l == "dhlecommerce" || l.starts_with("dhl") {
        return "DHL".to_string();
    }

    s.to_string()
}

async fn update_manual_shippo_shipments(db: &Database, company: &Company) -> Result<()> {
    // Only do this if the company is Oxide, as it is our account.
    if company.id != 1 {
        // Return early.
        return Ok(());
    }

    // Create the shippo client.
    let shippo = Shippo::new_from_env();

    // Get each of the shippo orders and create or update it in our set.
    // These are typically one off labels made from the UI.
    let orders = shippo.list_orders().await?;
    for order in orders {
        let mut ns = NewOutboundShipment {
            created_time: order.placed_at,
            name: order.to_address.name.to_string(),
            email: order.to_address.email.to_string(),
            phone: order.to_address.phone.to_string(),
            street_1: order.to_address.street1.to_string(),
            street_2: order.to_address.street2.to_string(),
            city: order.to_address.city.to_string(),
            state: order.to_address.state.to_string(),
            zipcode: order.to_address.zip.to_string(),
            country: order.to_address.country.to_string(),
            address_formatted: Default::default(),
            latitude: Default::default(),
            longitude: Default::default(),
            contents: "Manual internal shipment: could be swag or tools, etc".to_string(),
            carrier: Default::default(),
            pickup_date: None,
            delivered_time: None,
            shipped_time: None,
            provider: "Shippo".to_string(),
            provider_id: order.transactions.get(0).unwrap().object_id.to_string(),
            status: crate::shipment_status::Status::Queued.to_string(),
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
            cio_company_id: company.id,
        };

        // We need to get the carrier and tracking number so we don't create
        // duplicates every single time.
        let label = shippo.get_shipping_label(&ns.provider_id).await?;
        ns.tracking_number = label.tracking_number.to_string();

        // The rate will give us the carrier and the cost.
        let rate = shippo.get_rate(&label.rate).await?;
        ns.cost = rate.amount_local.parse()?;
        ns.carrier = clean_carrier_name(&rate.provider);

        // Only add the shipment if it doesn't already exist. Since we update it
        // in the loop above. Otherwise the email notifications get stuck and you get
        // innundated with notifications your package is on the way. Since it
        // thinks the status is always changing.
        let existing = OutboundShipment::get_from_db(db, ns.carrier.to_string(), ns.tracking_number.to_string()).await;
        if existing.is_some() {
            // We already have this shipment. Continue through our loop.
            continue;
        }

        // Upsert the record in the database.
        let mut s = ns.upsert_in_db(db).await?;

        // The shipment is actually new, lets send the notification for the status
        // as queued then.
        s.set_status(crate::shipment_status::Status::Queued).await?;

        // Update the shipment from shippo.
        s.create_or_get_shippo_shipment(db).await?;
        // Update airtable and the database again.
        s.update(db).await?;
    }

    Ok(())
}

// Sync the inbound shipments.
pub async fn refresh_inbound_shipments(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_shipments.is_empty() {
        // Return early.
        return Ok(());
    }

    let is: Vec<airtable_api::Record<InboundShipment>> = company
        .authenticate_airtable(&company.airtable_base_id_shipments)
        .list_records(&InboundShipment::airtable_table(), "Grid view", vec![])
        .await?;

    for record in is {
        if record.fields.carrier.is_empty() || record.fields.tracking_number.is_empty() {
            // Ignore it, it's a blank record.
            continue;
        }

        let mut new_shipment: NewInboundShipment = record.fields.into();
        new_shipment.expand().await?;
        new_shipment.cio_company_id = company.id;
        let mut shipment = new_shipment.upsert_in_db(db).await?;
        if shipment.airtable_record_id.is_empty() {
            shipment.airtable_record_id = record.id;
        }
        shipment.update(db).await?;
    }

    InboundShipments::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

pub fn clean_address_string(s: &str) -> String {
    if s == "DE" {
        return "Germany".to_string();
    } else if s == "GB" {
        return "Great Britian".to_string();
    }

    s.to_string()
}
