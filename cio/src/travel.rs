use chrono::{DateTime, Utc};
use macros::db;

use crate::airtable::AIRTABLE_BOOKINGS_TABLE;
use crate::companies::Company;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::bookings;

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
#[table_name = "bookings"]
pub struct NewBooking {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub booking_id: String,
    pub created_at: DateTime<Utc>,
    pub last_modified_at: DateTime<Utc>,
    #[serde(defaul, skip_serializing_if = "Option::is_none")]
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
    #[serde(defaul, skip_serializing_if = "Option::is_none")]
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
    pub optimal_price: f64,
    #[serde(default)]
    pub grand_total: f64,
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
    async fn update_airtable_record(&mut self, _record: Booking) {}
}

pub async fn refresh_trip_actions(db: &Database, company: &Company) {
    // Authenticate with TripActions.
    let ta = company.authenticate_tripactions(db).await;

    // Let's get our bookings.
    let bookings = ta.get_bookings().await.unwrap();
    for booking in bookings {
        println!("Booking: {:?}", booking);
    }
}

#[cfg(test)]
mod tests {
    use crate::companies::Company;
    use crate::db::Database;
    use crate::travel::refresh_trip_actions;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_travel() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_trip_actions(&db, &oxide).await;
    }
}
