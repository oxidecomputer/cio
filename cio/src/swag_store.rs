use anyhow::Result;
use chrono::Utc;
use log::info;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{companies::Company, db::Database, shipments::NewOutboundShipment, swag_inventory::SwagInventoryItem};

#[derive(Debug, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct Order {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
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
    /// This is who they know at the company.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OrderItem>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

#[derive(Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
pub struct OrderItem {
    /// This is the swag inventory item id.
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub quantity: i32,
}

impl Order {
    pub fn format_contents(&self) -> Result<String> {
        let db = Database::new();
        let mut contents = String::new();
        for item in &self.items {
            // Get the swag item from the database.
            let swag_inventory_item = SwagInventoryItem::get_by_id(&db, item.id)?;
            contents = format!(
                "{} x {}, Size: {}\n{}",
                item.quantity, swag_inventory_item.item, swag_inventory_item.size, contents
            );
        }

        Ok(contents.trim().to_string())
    }

    pub async fn create_shipment_for_order(&self, db: &Database) -> Result<()> {
        // Convert the shipment to an order.
        let shipment: NewOutboundShipment = self.clone().into();

        // Add the shipment to the database.
        let mut new_shipment = shipment.upsert_in_db(db)?;
        // Create or update the shipment from shippo.
        new_shipment.create_or_get_shippo_shipment(db).await?;
        // Update airtable and the database again.
        new_shipment.update(db).await?;

        // Send an email to the person that we recieved their order and what they are
        // getting.
        new_shipment.send_email_to_recipient_pre_shipping(db).await?;

        Ok(())
    }

    pub async fn subtract_order_from_inventory(&self, db: &Database) -> Result<()> {
        for item in &self.items {
            // Get the swag item from the database.
            let mut swag_inventory_item = SwagInventoryItem::get_by_id(db, item.id)?;
            let mut new = swag_inventory_item.current_stock - item.quantity;
            if swag_inventory_item.current_stock < 0 {
                // TODO: Hopefully this never happens. The store code _should_ only allow people
                // to order what is in stock but just in case let's make sure this does not
                // go negative.
                new = 0;
            }

            let company = swag_inventory_item.company(db)?;

            // This will also set the value.
            swag_inventory_item
                .send_slack_notification_if_inventory_changed(db, &company, new)
                .await?;

            info!(
                "subtracted `{}` from current stock of `{}` making the total now `{}`",
                item.quantity, swag_inventory_item.name, swag_inventory_item.current_stock
            );
            swag_inventory_item.update(db).await?;
        }

        Ok(())
    }

    pub async fn do_order(&self, db: &Database) -> Result<()> {
        // If their email is empty return early.
        if self.email.is_empty()
            || self.street_1.is_empty()
            || self.city.is_empty()
            || self.state.is_empty()
            || self.zipcode.is_empty()
            || self.phone.is_empty()
            || self.name.is_empty()
            || self.items.is_empty()
        {
            // This should not happen since we verify on the client side we have these
            // things.
            return Ok(());
        }

        self.create_shipment_for_order(db).await?;
        self.subtract_order_from_inventory(db).await?;

        Ok(())
    }
}

impl From<Order> for NewOutboundShipment {
    fn from(order: Order) -> Self {
        let db = Database::new();
        let company = Company::get_by_id(&db, order.cio_company_id).unwrap();

        NewOutboundShipment {
            created_time: Utc::now(),
            name: order.name.to_string(),
            email: order.email.to_string(),
            phone: order.phone.to_string(),
            street_1: order.street_1.to_string(),
            street_2: order.street_2.to_string(),
            city: order.city.to_string(),
            state: order.state.to_string(),
            zipcode: order.zipcode.to_string(),
            country: order.country.to_string(),
            notes: format!(
                "Automatically generated order from the {} store. \"Who do you know at {}?\" {}",
                company.name, company.name, order.notes
            ),
            // This will be populated when we update shippo.
            address_formatted: Default::default(),
            latitude: Default::default(),
            longitude: Default::default(),
            contents: order.format_contents().unwrap(),
            // The rest will be populated when we update shippo and create a label.
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
            geocode_cache: Default::default(),
            local_pickup: false,
            link_to_package_pickup: Default::default(),
            cio_company_id: order.cio_company_id,
        }
    }
}
