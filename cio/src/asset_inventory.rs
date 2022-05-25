use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use barcoders::{
    generators::{image::Image, svg::SVG},
    sym::code39::Code39,
};
use google_drive::{
    traits::{DriveOps, FileOps},
    Client as GoogleDrive,
};
use log::warn;
use macros::db;
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::AIRTABLE_ASSET_ITEMS_TABLE, companies::Company, core::UpdateAirtableRecord, db::Database,
    schema::asset_items, swag_inventory::generate_pdf_barcode_label,
};

#[db {
    new_struct_name = "AssetItem",
    airtable_base = "assets",
    airtable_table = "AIRTABLE_ASSET_ITEMS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = asset_items)]
pub struct NewAssetItem {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
    pub picture: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualities: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub serial_number: String,
    #[serde(default)]
    pub purchase_price: f32,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub current_employee_borrowing: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conference_room_using: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,

    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::barcode_format_as_string::serialize",
        deserialize_with = "airtable_api::barcode_format_as_string::deserialize"
    )]
    pub barcode: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
    pub barcode_png: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
    pub barcode_svg: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
    pub barcode_pdf_label: String,

    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a AssetItem.
#[async_trait]
impl UpdateAirtableRecord<AssetItem> for AssetItem {
    async fn update_airtable_record(&mut self, _record: AssetItem) -> Result<()> {
        Ok(())
    }
}

impl NewAssetItem {
    pub fn generate_barcode(&self) -> String {
        let mut barcode = self
            .name
            .to_uppercase()
            .replace(' ', "")
            .replace('/', "")
            .replace('(', "")
            .replace(')', "")
            .replace('-', "")
            .replace('\'', "")
            .trim()
            .to_string();

        // Add zeros to start of barcode til it is 39 chars long.
        // This makes sure the barcodes are all of uniform length.
        // To fit on the barcode label with the right DPI we CANNOT exceed this
        // legth.
        let max_barcode_len = 20;
        while barcode.len() < max_barcode_len {
            barcode = format!("0{}", barcode);
        }
        if barcode.len() > max_barcode_len {
            warn!(
                "len too long {} {}, needs to be {} or under",
                barcode,
                barcode.len(),
                max_barcode_len
            );
        }

        barcode
    }

    pub async fn generate_barcode_images(
        &mut self,
        drive_client: &GoogleDrive,
        drive_id: &str,
        parent_id: &str,
    ) -> Result<String> {
        let barcode_value = self.generate_barcode();

        // if the barcode value has changed since the last time it was generated then we
        // need to regenerate the barcode images. Additionally if we do not have values
        // stored for any of the barcode images, then we need to generate
        if self.barcode != barcode_value
            || self.barcode_png.is_empty()
            || self.barcode_svg.is_empty()
            || self.barcode_pdf_label.is_empty()
        {
            // Generate the barcode.
            // "Name" is automatically generated by Airtable from the item and the size.
            if self.name.is_empty() {
                // Return early.
                return Ok(String::new());
            }

            // Store the updated value after checking that the name field exists. This ensures that
            // we do not store a barcode value and then fail to generate images
            self.barcode = barcode_value;

            // Generate the barcode svg and png.
            let barcode = Code39::new(&self.barcode)?;
            let png = Image::png(60); // You must specify the height in pixels.
            let encoded = barcode.encode();

            // Image generators return a Result<Vec<u8>, barcoders::error::Error) of encoded bytes.
            let png_bytes = png.generate(&encoded[..])?;
            let mut file_name = format!("{} {}.png", self.type_, self.name.replace('/', ""));

            // Create or update the file in the google drive.
            let png_file = drive_client
                .files()
                .create_or_update(drive_id, parent_id, &file_name, "image/png", &png_bytes)
                .await?;
            self.barcode_png = format!("https://drive.google.com/uc?export=download&id={}", png_file.id);

            // Now do the SVG.
            let svg = SVG::new(200); // You must specify the height in pixels.
            let svg_data: String = svg.generate(&encoded)?;
            let svg_bytes = svg_data.as_bytes();

            file_name = format!("{} {}.svg", self.type_, self.name.replace('/', ""));

            // Create or update the file in the google drive.
            let svg_file = drive_client
                .files()
                .create_or_update(drive_id, parent_id, &file_name, "image/svg+xml", svg_bytes)
                .await?;
            self.barcode_svg = format!("https://drive.google.com/uc?export=download&id={}", svg_file.id);

            // Generate the barcode label.
            let im = Image::jpeg(400);
            let b = im.generate(&encoded[..])?;
            let label_bytes = generate_pdf_barcode_label(
                &b,
                &self.barcode,
                &self.name,
                &format!("{} {} {}", self.manufacturer, self.type_, self.model_number),
            )?;
            file_name = format!("{} {} - Barcode Label.pdf", self.type_, self.name.replace('/', ""));
            // Create or update the file in the google drive.
            let label_file = drive_client
                .files()
                .create_or_update(drive_id, parent_id, &file_name, "application/pdf", &label_bytes)
                .await?;
            self.barcode_pdf_label = format!("https://drive.google.com/uc?export=download&id={}", label_file.id);
        }

        Ok(self.barcode_pdf_label.to_string())
    }

    pub async fn expand(&mut self, drive_client: &GoogleDrive, drive_id: &str, parent_id: &str) -> Result<String> {
        self.generate_barcode_images(drive_client, drive_id, parent_id).await
    }
}

/// A request to print labels.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct PrintLabelsRequest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub quantity: i32,
}

impl AssetItem {
    /// Send the label to our printer.
    pub async fn print_label(&self, db: &Database) -> Result<()> {
        let company = self.company(db).await?;

        if company.printer_url.is_empty() {
            // Return early.
            return Ok(());
        }

        let url = if self.barcode_pdf_label.trim().is_empty() {
            // Get the URL to the google item directly.
            // Initialize the Google Drive client.
            let drive_client = company.authenticate_google_drive(db).await?;

            // Figure out where our directory is.
            // It should be in the shared drive : "Automated Documents"/"rfds"
            let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
            let drive_id = shared_drive.id.to_string();

            // Get the directory by the name.
            let parent_id = drive_client.files().create_folder(&drive_id, "", "assets").await?;

            let mut sw: NewAssetItem = From::from(self.clone());
            sw.expand(&drive_client, &drive_id, &parent_id).await?
        } else {
            self.barcode_pdf_label.trim().to_string()
        };

        let printer_url = format!("{}/zebra", company.printer_url);
        let client = reqwest::Client::new();
        let resp = client
            .post(&printer_url)
            .body(json!(PrintLabelsRequest { url, quantity: 1 }).to_string())
            .send()
            .await?;
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => {
                bail!("print zebra status_code: {}, body: {}", s, resp.text().await?);
            }
        };

        Ok(())
    }
}

/// Sync asset items from Airtable.
pub async fn refresh_asset_items(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_assets.is_empty() {
        // Return early.
        return Ok(());
    }

    // Initialize the Google Drive client.
    let mut drive_client = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let parent_id = drive_client.files().create_folder(&drive_id, "", "assets").await?;

    // Get all the records from Airtable.
    let mut generator = names::Generator::default();
    let results: Vec<airtable_api::Record<AssetItem>> = company
        .authenticate_airtable(&company.airtable_base_id_assets)
        .list_records(&AssetItem::airtable_table(), "Grid view", vec![])
        .await?;
    for item_record in results {
        let mut item: NewAssetItem = item_record.fields.into();
        if item.name.is_empty() {
            item.name = generator.next().unwrap();
        }

        // Iterating through and processing all of the asset items can take over an hour. This
        // exceeds the time limit that Google Drive allots for a single token. Therefore we may
        // need to refresh the access token mid processing if an item expansion fails
        match item.expand(&drive_client, &drive_id, &parent_id).await {
            Ok(_) => (),
            Err(err) => {
                log::info!("Handling drive error. This is likely to be an authentication error. Further work is needed to differentiate. {:?}", err);
                log::info!("Reauthenticating with Google Drive");
                drive_client = company.authenticate_google_drive(db).await?;

                // Now using a client with fresh credentials, we can retry the expansion. If this
                // again, it is unlikely due to an authentication error
                item.expand(&drive_client, &drive_id, &parent_id).await?;
            }
        }

        item.cio_company_id = company.id;

        let mut db_item = item.upsert_in_db(db).await?;
        db_item.airtable_record_id = item_record.id.to_string();
        db_item.update(db).await?;
    }

    AssetItems::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}
