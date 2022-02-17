use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_BOOKINGS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database, schema::bookings,
};

#[db {
    new_struct_name = "Booking",
    airtable_base = "travel",
    airtable_table = "AIRTABLE_BOOKINGS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "booking_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = bookings)]
pub struct NewBooking {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub booking_id: String,
    pub created_at: DateTime<Utc>,
    pub last_modified_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flight: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cabin: String,
    #[serde(default)]
    pub is_preferred_vendor: bool,
    #[serde(default)]
    pub used_corporate_discount: bool,
    pub start_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<NaiveDate>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "airtable_api::user_format_as_array_of_strings::serialize",
        deserialize_with = "airtable_api::user_format_as_array_of_strings::deserialize"
    )]
    pub passengers: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub booker: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub destination: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub length: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(default)]
    pub optimal_price: f32,
    #[serde(default)]
    pub grand_total: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub purpose: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confirmation_id: String,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a Booking.
#[async_trait]
impl UpdateAirtableRecord<Booking> for Booking {
    async fn update_airtable_record(&mut self, _record: Booking) -> Result<()> {
        Ok(())
    }
}

pub async fn refresh_trip_actions(db: &Database, company: &Company) -> Result<()> {
    // Authenticate with TripActions.
    let tripactions_auth = company.authenticate_tripactions(db).await;
    if let Err(e) = tripactions_auth {
        if e.to_string().contains("no token") {
            // Return early, this company does not use TripActions.
            return Ok(());
        }

        bail!("authenticating tripactions failed: {}", e);
    }
    let ta = tripactions_auth?;

    // Let's get our bookings.
    let bookings = ta
        .booking_data()
        .get_all_booking_report(
            &Utc::now()
                .checked_sub_signed(Duration::weeks(52))
                .unwrap()
                .timestamp()
                .to_string(), // created from
            &Utc::now().timestamp().to_string(), // created to
            "",                                  // start_date_from
            "",                                  // start_date_to
            tripactions::types::BookingStatus::Noop,
            tripactions::types::BookingType::Noop,
        )
        .await?;

    for booking in bookings {
        // Create our list of passengers.
        let mut passengers: Vec<String> = Default::default();
        for passenger in &booking.passengers {
            passengers.push(passenger.person.as_ref().unwrap().email.to_string());
        }

        let b = NewBooking {
            booking_id: booking.uuid.to_string(),
            created_at: booking.created.unwrap(),
            last_modified_at: booking.last_modified.unwrap(),
            cancelled_at: booking.cancelled_at,
            // TODO: add the cancellation reason? we have it in tripactions.
            type_: booking.booking_type.unwrap().to_string(),
            status: booking.booking_status.to_string(),
            vendor: booking.vendor.to_string(),
            flight: booking.flight.to_string(),
            cabin: booking.cabin.to_string(),
            is_preferred_vendor: booking.preferred_vendor == "Y",
            used_corporate_discount: booking.corporate_discount_used == "Y",
            start_date: booking.start_date.unwrap(),
            end_date: booking.end_date,
            passengers,
            booker: booking.booker.unwrap().email.to_string(),
            origin: booking.origin.unwrap().airport_code.to_string(),
            destination: booking.destination.unwrap().airport_code.to_string(),
            length: booking.trip_length.to_string(),
            description: booking.trip_description.to_string(),
            currency: booking.currency,
            optimal_price: booking.optimal_price as f32,
            grand_total: booking.grand_total as f32,
            purpose: booking.purpose.to_string(),
            reason: booking.reason.to_string(),
            confirmation_id: booking.booking_id.to_string(),
            cio_company_id: company.id,
        };
        b.upsert(db).await?;
    }

    Ok(())
}
