use std::io::BufWriter;

use async_trait::async_trait;
use barcoders::{
    generators::{image::*, svg::*},
    sym::code39::*,
};
use chrono::{DateTime, Utc};
use google_drive::GoogleDrive;
use macros::db;
use printpdf::{
    types::plugins::graphics::two_dimensional::image::Image as PdfImage, Mm, PdfDocument, Pt,
};
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    airtable::{
        AIRTABLE_BARCODE_SCANS_TABLE, AIRTABLE_SWAG_INVENTORY_ITEMS_TABLE,
        AIRTABLE_SWAG_ITEMS_TABLE,
    },
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{barcode_scans, swag_inventory_items, swag_items},
};

// The zebra label printer's dpi is 320.
const DPI: f64 = 320.0;

#[db {
    new_struct_name = "SwagItem",
    airtable_base = "swag",
    airtable_table = "AIRTABLE_SWAG_ITEMS_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "swag_items"]
pub struct NewSwagItem {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_string::deserialize"
    )]
    pub image: String,
    #[serde(default)]
    pub internal_only: bool,

    /// This is populated by Airtable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_inventory: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_barcode_scans: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_order_january_2020: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_order_october_2020: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_order_may_2021: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a SwagItem.
#[async_trait]
impl UpdateAirtableRecord<SwagItem> for SwagItem {
    async fn update_airtable_record(&mut self, record: SwagItem) {
        if !record.link_to_inventory.is_empty() {
            self.link_to_inventory = record.link_to_inventory;
        }
        if !record.link_to_barcode_scans.is_empty() {
            self.link_to_barcode_scans = record.link_to_barcode_scans;
        }
    }
}

/// Sync swag items from Airtable.
pub async fn refresh_swag_items(db: &Database, company: &Company) {
    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagItem>> = company
        .authenticate_airtable(&company.airtable_base_id_swag)
        .list_records(&SwagItem::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
    for item_record in results {
        let mut item: NewSwagItem = item_record.fields.into();
        item.cio_company_id = company.id;

        let mut db_item = item.upsert_in_db(db);
        db_item.airtable_record_id = item_record.id.to_string();
        db_item.update(db).await;
    }
}

#[db {
    new_struct_name = "SwagInventoryItem",
    airtable_base = "swag",
    airtable_table = "AIRTABLE_SWAG_INVENTORY_ITEMS_TABLE",
    match_on = {
        "item" = "String",
        "size" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "swag_inventory_items"]
pub struct NewSwagInventoryItem {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub size: String,
    #[serde(default)]
    pub current_stock: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub item: String,
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

    /// The quantity of labels to print.
    /// This field will be set and updated in Airtable.
    #[serde(default)]
    pub print_barcode_label_quantity: i32,

    /// This is populated by Airtable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_item: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a SwagInventoryItem.
#[async_trait]
impl UpdateAirtableRecord<SwagInventoryItem> for SwagInventoryItem {
    async fn update_airtable_record(&mut self, record: SwagInventoryItem) {
        if !record.link_to_item.is_empty() {
            self.link_to_item = record.link_to_item;
        }

        // This is a funtion in Airtable so we can't update it.
        self.name = "".to_string();

        // This is set in airtable so we need to keep it.
        self.print_barcode_label_quantity = record.print_barcode_label_quantity;
    }
}

impl NewSwagInventoryItem {
    pub fn generate_barcode(&mut self) {
        let mut barcode = self
            .name
            .to_uppercase()
            .replace("FIRST EDITION", "1ED")
            .replace("SECOND EDITION", "2ED")
            .replace("THIRD EDITION", "3ED")
            // TODO: Find another way to do this so that it doesn't break eventually.
            .replace("FOURTH EDITION", "4ED")
            .replace(' ', "")
            .replace('/', "")
            .replace('(', "")
            .replace(')', "")
            .replace('-', "")
            .replace("'", "")
            .replace("UNISEX", "U")
            .replace("WOMENS", "W")
            .replace("MENS", "M")
            .replace("TODDLERS", "T")
            .replace("YOUTH", "Y")
            .replace("ONESIE", "B")
            .replace("MOLESKINE", "MS")
            .replace("NOTEBOOK", "NB")
            .replace("TEE", "T")
            .replace("DIGITALCOMPUTER", "DEC")
            .replace("TURBOBUTTON", "TURBO")
            .replace("HOODIE", "HOOD")
            .replace("RAMBLER", "R")
            .replace("RULED", "RULE")
            .trim()
            .to_string();

        // Add zeros to start of barcode til it is 39 chars long.
        // This makes sure the barcodes are all of uniform length.
        // To fit on the barcode label with the right DPI we CANNOT exceed this
        // legth.
        let max_barcode_len = 13;
        while barcode.len() < max_barcode_len {
            barcode = format!("0{}", barcode);
        }
        if barcode.len() > max_barcode_len {
            println!(
                "len too long {} {}, needs to be {} or under",
                barcode,
                barcode.len(),
                max_barcode_len
            );
        }

        self.barcode = barcode;
    }

    pub async fn generate_barcode_images(
        &mut self,
        drive_client: &GoogleDrive,
        drive_id: &str,
        parent_id: &str,
    ) {
        // Generate the barcode.
        // "Name" is automatically generated by Airtable from the item and the size.
        if !self.name.is_empty() {
            // Generate the barcode svg and png.
            let barcode = Code39::new(&self.barcode).unwrap();
            let png = Image::png(45); // You must specify the height in pixels.
            let encoded = barcode.encode();

            // Image generators return a Result<Vec<u8>, barcoders::error::Error) of encoded bytes.
            let png_bytes = png.generate(&encoded[..]).unwrap();
            let mut file_name = format!("{}.png", self.name.replace('/', ""));

            // Create or update the file in the google drive.
            let png_file = drive_client
                .create_or_update_file(drive_id, parent_id, &file_name, "image/png", &png_bytes)
                .await
                .unwrap();
            self.barcode_png = format!(
                "https://drive.google.com/uc?export=download&id={}",
                png_file.id
            );

            // Now do the SVG.
            let svg = SVG::new(200); // You must specify the height in pixels.
            let svg_data: String = svg.generate(&encoded).unwrap();
            let svg_bytes = svg_data.as_bytes();

            file_name = format!("{}.svg", self.name.replace('/', ""));

            // Create or update the file in the google drive.
            let svg_file = drive_client
                .create_or_update_file(drive_id, parent_id, &file_name, "image/svg+xml", svg_bytes)
                .await
                .unwrap();
            self.barcode_svg = format!(
                "https://drive.google.com/uc?export=download&id={}",
                svg_file.id
            );

            // Generate the barcode label.
            let label_bytes = generate_pdf_barcode_label(
                &png_bytes,
                &self.barcode,
                &self.item,
                &format!("Size: {}", self.size),
            );
            file_name = format!("{} - Barcode Label.pdf", self.name.replace('/', ""));
            // Create or update the file in the google drive.
            let label_file = drive_client
                .create_or_update_file(
                    drive_id,
                    parent_id,
                    &file_name,
                    "application/pdf",
                    &label_bytes,
                )
                .await
                .unwrap();
            self.barcode_pdf_label = format!(
                "https://drive.google.com/uc?export=download&id={}",
                label_file.id
            );
        }
    }

    pub async fn expand(&mut self, drive_client: &GoogleDrive, drive_id: &str, parent_id: &str) {
        self.generate_barcode();
        self.generate_barcode_images(drive_client, drive_id, parent_id)
            .await;
    }
}

// Get the bytes for a pdf barcode label.
pub fn generate_pdf_barcode_label(
    png_bytes: &[u8],
    text_line_1: &str,
    text_line_2: &str,
    text_line_3: &str,
) -> Vec<u8> {
    let pdf_margin = Mm(2.0);
    let pdf_width = Mm(3.0 * 25.4);
    let pdf_height = Mm(2.0 * 25.4);
    let (doc, page1, layer1) = PdfDocument::new(text_line_2, pdf_width, pdf_height, "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // currently, the only reliable file formats are bmp/jpeg/png
    // this is an issue of the image library, not a fault of printpdf
    let logo_bytes = include_bytes!("oxide_logo.png");
    let logo_image = PdfImage::from_dynamic_image(&image::load_from_memory(logo_bytes).unwrap());

    // We want the logo width to fit.
    let original_width = logo_image.image.width.into_pt(DPI);
    let new_width: Pt = (pdf_width - (pdf_margin * 2.0)).into();
    let width_scale = new_width / original_width;
    let logo_height: Pt = (logo_image.image.height.into_pt(DPI) * width_scale).into();
    let logo_height_mm: Mm = From::from(logo_height);
    // translate x, translate y, rotate, scale x, scale y
    // rotations and translations are always in relation to the lower left corner
    logo_image.add_to_layer(
        current_layer.clone(),
        Some(pdf_margin),
        Some(pdf_height - pdf_margin - logo_height_mm),
        None,
        Some(width_scale),
        Some(width_scale),
        Some(DPI),
    );

    let barcode_image = PdfImage::from_dynamic_image(&image::load_from_memory(&png_bytes).unwrap());
    // We want the barcode width to fit.
    let original_width = barcode_image.image.width.into_pt(DPI);
    let new_width: Pt = (pdf_width - (pdf_margin * 2.0)).into();
    let width_scale = new_width / original_width;
    // translate x, translate y, rotate, scale x, scale y
    // rotations and translations are always in relation to the lower left corner
    barcode_image.add_to_layer(
        current_layer.clone(),
        Some(pdf_margin),
        Some(pdf_height - (pdf_margin * 2.0) - logo_height_mm),
        None,
        Some(width_scale),
        Some(width_scale),
        Some(DPI),
    );

    let font_bytes = include_bytes!("Inconsolata/Inconsolata-Regular.ttf").to_vec();
    let font = doc.add_external_font(&*font_bytes).unwrap();

    // For more complex layout of text, you can use functions
    // defined on the PdfLayerReference
    // Make sure to wrap your commands
    // in a `begin_text_section()` and `end_text_section()` wrapper
    current_layer.begin_text_section();

    current_layer.set_font(&font, 12.0);
    let h = Pt(14.0 * 3.0);
    let hmm: Mm = From::from(h);
    current_layer.set_text_cursor(pdf_margin, pdf_margin + hmm);
    current_layer.set_line_height(14.0);

    current_layer.write_text(text_line_1.clone(), &font);
    current_layer.add_line_break();
    current_layer.write_text(text_line_2.clone(), &font);
    current_layer.add_line_break();
    current_layer.write_text(text_line_3.clone(), &font);
    current_layer.add_line_break();

    current_layer.end_text_section();

    // Save the PDF
    let mut bw = BufWriter::new(Vec::new());

    doc.save(&mut bw).unwrap();

    bw.into_inner().unwrap()
}

/// A request to print labels.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct PrintLabelsRequest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub quantity: i32,
}

impl SwagInventoryItem {
    /// Send the label to our printer.
    pub async fn print_label(&self, db: &Database) {
        if self.barcode_pdf_label.trim().is_empty() {
            // Return early.
            return;
        }

        let company = self.company(db);

        if company.printer_url.is_empty() {
            // Return early.
            return;
        }

        let printer_url = format!("{}/zebra", company.printer_url);
        let client = reqwest::Client::new();
        let resp = client
            .post(&printer_url)
            .body(
                json!(PrintLabelsRequest {
                    url: self.barcode_pdf_label.to_string(),
                    quantity: self.print_barcode_label_quantity
                })
                .to_string(),
            )
            .send()
            .await
            .unwrap();
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => {
                panic!(
                    "[print]: status_code: {}, body: {}",
                    s,
                    resp.text().await.unwrap()
                );
            }
        };
    }
}

/// Sync swag inventory items from Airtable.
pub async fn refresh_swag_inventory_items(db: &Database, company: &Company) {
    // Get gsuite token.
    let token = company.authenticate_google(db).await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client
        .get_drive_by_name("Automated Documents")
        .await
        .unwrap();
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let drive_rfd_dir = drive_client
        .get_file_by_name(&drive_id, "swag")
        .await
        .unwrap();
    let parent_id = drive_rfd_dir.get(0).unwrap().id.to_string();

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagInventoryItem>> = company
        .authenticate_airtable(&company.airtable_base_id_swag)
        .list_records(&SwagInventoryItem::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
    for inventory_item_record in results {
        let mut inventory_item: NewSwagInventoryItem = inventory_item_record.fields.into();
        inventory_item
            .expand(&drive_client, &drive_id, &parent_id)
            .await;
        inventory_item.cio_company_id = company.id;

        let mut db_inventory_item = inventory_item.upsert_in_db(db);
        db_inventory_item.airtable_record_id = inventory_item_record.id.to_string();
        db_inventory_item.update(db).await;
    }
}

#[db {
    new_struct_name = "BarcodeScan",
    airtable_base = "swag",
    airtable_table = "AIRTABLE_BARCODE_SCANS_TABLE",
    match_on = {
        "item" = "String",
        "size" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "barcode_scans"]
pub struct NewBarcodeScan {
    pub time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub size: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub item: String,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::barcode_format_as_string::serialize",
        deserialize_with = "airtable_api::barcode_format_as_string::deserialize"
    )]
    pub barcode: String,

    /// This is populated by Airtable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_item: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a BarcodeScan.
#[async_trait]
impl UpdateAirtableRecord<BarcodeScan> for BarcodeScan {
    async fn update_airtable_record(&mut self, _record: BarcodeScan) {}
}

impl BarcodeScan {
    // Takes a scanned barcode and updates the inventory count for the item
    // as well as adds the scan to the barcodes_scan table for tracking.
    pub async fn scan(b: String) {
        let time = Utc::now();

        // Make sure the barcode is formatted correctly.
        let barcode = b.trim().to_uppercase().to_string();

        // Initialize the database connection.
        let db = Database::new();

        // Firstly, let's make sure we have the barcode in the database.
        match swag_inventory_items::dsl::swag_inventory_items
            .filter(swag_inventory_items::dsl::barcode.eq(barcode.to_string()))
            .first::<SwagInventoryItem>(&db.conn())
        {
            Ok(mut swag_inventory_item) => {
                // We found the matching inventory item!
                // Now let's subtract 1 from the current inventory and update it
                // in the database.
                swag_inventory_item.current_stock -= 1;
                // Update the database.
                swag_inventory_item.update(&db).await;
                println!(
                    "Subtracted one from {} stock, we now have {}",
                    swag_inventory_item.name, swag_inventory_item.current_stock
                );

                // Now add our barcode scan to the barcode scans database.
                let new_barcode_scan = NewBarcodeScan {
                    time,
                    item: swag_inventory_item.item.to_string(),
                    size: swag_inventory_item.size.to_string(),
                    link_to_item: swag_inventory_item.link_to_item,
                    barcode: barcode.to_string(),
                    name: swag_inventory_item.name.to_string(),
                    cio_company_id: swag_inventory_item.cio_company_id,
                };

                // Add our barcode scan to the database.
                new_barcode_scan.upsert(&db).await;
            }
            Err(e) => println!(
                "could not find inventory item with barcode {}: {}",
                barcode, e
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        companies::Company,
        db::Database,
        swag_inventory::{
            refresh_swag_inventory_items, refresh_swag_items, BarcodeScans, SwagInventoryItems,
            SwagItems,
        },
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_swag_items() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_swag_items(&db, &oxide).await;
        SwagItems::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_swag_inventory_items() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_swag_inventory_items(&db, &oxide).await;
        SwagInventoryItems::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_swag_inventory_refresh_barcode_scans() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        BarcodeScans::get_from_db(&db, oxide.id)
            .update_airtable(&db)
            .await;
    }
}
