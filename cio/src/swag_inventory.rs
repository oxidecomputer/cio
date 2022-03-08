use std::io::BufWriter;

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use barcoders::{
    generators::{image::Image, svg::SVG},
    sym::code39::Code39,
};
use chrono::{DateTime, Utc};
use google_drive::{
    traits::{DriveOps, FileOps},
    Client as GoogleDrive,
};
use log::{info, warn};
use macros::db;
use printpdf::{Image as PdfImage, Mm, PdfDocument, Pt};
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockAccessory, MessageBlockText, MessageBlockType,
    MessageType,
};

use crate::{
    airtable::{AIRTABLE_BARCODE_SCANS_TABLE, AIRTABLE_SWAG_INVENTORY_ITEMS_TABLE, AIRTABLE_SWAG_ITEMS_TABLE},
    companies::Company,
    core::UpdateAirtableRecord,
    db::Database,
    schema::{barcode_scans, swag_inventory_items, swag_items},
};

// The zebra label printer's dpi is 300.
const DPI: f64 = 300.0;

#[db {
    new_struct_name = "SwagItem",
    airtable_base = "swag",
    airtable_table = "AIRTABLE_SWAG_ITEMS_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = swag_items)]
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
    #[tracing::instrument]
    async fn update_airtable_record(&mut self, record: SwagItem) -> Result<()> {
        if !record.link_to_inventory.is_empty() {
            self.link_to_inventory = record.link_to_inventory;
        }
        if !record.link_to_barcode_scans.is_empty() {
            self.link_to_barcode_scans = record.link_to_barcode_scans;
        }

        Ok(())
    }
}

/// Sync swag items from Airtable.
pub async fn refresh_swag_items(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_swag.is_empty() {
        // Return early.
        return Ok(());
    }

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagItem>> = company
        .authenticate_airtable(&company.airtable_base_id_swag)
        .list_records(&SwagItem::airtable_table(), "Grid view", vec![])
        .await?;
    for item_record in results {
        let mut item: NewSwagItem = item_record.fields.into();
        item.cio_company_id = company.id;

        let mut db_item = item.upsert_in_db(db).await?;
        db_item.airtable_record_id = item_record.id.to_string();
        db_item.update(db).await?;
    }

    SwagItems::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
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
#[diesel(table_name = swag_inventory_items)]
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
    #[tracing::instrument]
    async fn update_airtable_record(&mut self, record: SwagInventoryItem) -> Result<()> {
        if !record.link_to_item.is_empty() {
            self.link_to_item = record.link_to_item;
        }

        // This is a funtion in Airtable so we can't update it.
        self.name = "".to_string();

        // This is set in airtable so we need to keep it.
        self.print_barcode_label_quantity = record.print_barcode_label_quantity;

        Ok(())
    }
}

impl NewSwagInventoryItem {
    #[tracing::instrument]
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let mut msg: FormattedMessage = self.clone().into();
        // Set the channel.
        msg.channel = company.slack_channel_swag.to_string();
        // Post the message.
        company.post_to_slack_channel(db, &msg).await?;

        Ok(())
    }

    #[tracing::instrument]
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
            .replace('\'', "")
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
            .replace("SWEATSHIRT", "SWS")
            .replace("RAMBLER", "R")
            .replace("RULED", "RULE")
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

        self.barcode = barcode;
    }

    #[tracing::instrument(skip(drive_client))]
    pub async fn generate_barcode_images(
        &mut self,
        drive_client: &GoogleDrive,
        drive_id: &str,
        parent_id: &str,
    ) -> Result<String> {
        // Generate the barcode.
        // "Name" is automatically generated by Airtable from the item and the size.
        if self.name.is_empty() {
            // Return early.
            return Ok(String::new());
        }

        // Generate the barcode svg and png.
        let barcode = Code39::new(&self.barcode)?;
        let png = Image::png(60); // You must specify the height in pixels.
        let encoded = barcode.encode();

        // Image generators return a Result<Vec<u8>, barcoders::error::Error) of encoded bytes.
        let png_bytes = png.generate(&encoded[..])?;
        let mut file_name = format!("{}.png", self.name.replace('/', ""));

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

        file_name = format!("{}.svg", self.name.replace('/', ""));

        // Create or update the file in the google drive.
        let svg_file = drive_client
            .files()
            .create_or_update(drive_id, parent_id, &file_name, "image/svg+xml", svg_bytes)
            .await?;
        self.barcode_svg = format!("https://drive.google.com/uc?export=download&id={}", svg_file.id);

        // Generate the barcode label.
        let im = Image::jpeg(400);
        let b = im.generate(&encoded[..])?;
        let label_bytes = generate_pdf_barcode_label(&b, &self.barcode, &self.item, &format!("Size: {}", self.size))?;
        file_name = format!("{} - Barcode Label.pdf", self.name.replace('/', ""));
        // Create or update the file in the google drive.
        let label_file = drive_client
            .files()
            .create_or_update(drive_id, parent_id, &file_name, "application/pdf", &label_bytes)
            .await?;
        self.barcode_pdf_label = format!("https://drive.google.com/uc?export=download&id={}", label_file.id);

        Ok(self.barcode_pdf_label.to_string())
    }

    #[tracing::instrument(skip(drive_client))]
    pub async fn expand(&mut self, drive_client: &GoogleDrive, drive_id: &str, parent_id: &str) -> Result<String> {
        self.generate_barcode();
        self.generate_barcode_images(drive_client, drive_id, parent_id).await
    }
}

/// Convert the swag inventory item into a Slack message.
impl From<NewSwagInventoryItem> for FormattedMessage {
    #[tracing::instrument]
    fn from(item: NewSwagInventoryItem) -> Self {
        let text = format!("*{}*\n | current stock: {}", item.name, item.current_stock);

        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: "".to_string(),
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
                        block_type: MessageBlockType::Section,
                        text: Some(MessageBlockText {
                            text_type: MessageType::Markdown,
                            text,
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
                            text: format!("Swag inventory item | {} | {}", item.item, item.size),
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

impl From<SwagInventoryItem> for FormattedMessage {
    #[tracing::instrument]
    fn from(item: SwagInventoryItem) -> Self {
        let new: NewSwagInventoryItem = item.into();
        new.into()
    }
}

impl SwagInventoryItem {
    #[tracing::instrument]
    pub async fn send_slack_notification(&self, db: &Database, company: &Company) -> Result<()> {
        let n: NewSwagInventoryItem = self.into();
        n.send_slack_notification(db, company).await
    }
}

// Get the bytes for a pdf barcode label.
pub fn generate_pdf_barcode_label(
    image_bytes: &[u8],
    text_line_1: &str,
    text_line_2: &str,
    text_line_3: &str,
) -> Result<Vec<u8>> {
    let pdf_margin = Mm(1.0);
    let pdf_width = Mm(3.0 * 25.4);
    let pdf_height = Mm(2.0 * 25.4);
    let (doc, page1, layer1) = PdfDocument::new(text_line_2, pdf_width, pdf_height, "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // currently, the only reliable file formats are bmp/jpeg/png
    // this is an issue of the image library, not a fault of printpdf
    let logo_bytes = include_bytes!("oxide_logo.png");
    let logo_image = PdfImage::from_dynamic_image(&image::load_from_memory(logo_bytes)?);

    // We want the logo width to fit.
    let original_width = logo_image.image.width.into_pt(DPI);
    let new_width: Pt = (pdf_width - (pdf_margin * 2.0)).into();
    let width_scale = new_width / original_width;
    let logo_height: Pt = logo_image.image.height.into_pt(DPI) * width_scale;
    let logo_height_mm: Mm = From::from(logo_height);
    // translate x, translate y, rotate, scale x, scale y
    // rotations and translations are always in relation to the lower left corner
    logo_image.add_to_layer(
        current_layer.clone(),
        printpdf::ImageTransform {
            translate_x: Some(pdf_margin),
            translate_y: Some(pdf_height - pdf_margin - logo_height_mm),
            rotate: None,
            scale_x: Some(width_scale),
            scale_y: Some(width_scale),
            dpi: Some(DPI),
        },
    );

    let line_height = 12.0;
    let h = Pt(line_height * 2.0);
    let hmm: Mm = From::from(h);

    let font_bytes = include_bytes!("Inconsolata/Inconsolata-Regular.ttf").to_vec();
    let font = doc.add_external_font(&*font_bytes)?;

    // For more complex layout of text, you can use functions
    // defined on the PdfLayerReference
    // Make sure to wrap your commands
    // in a `begin_text_section()` and `end_text_section()` wrapper
    current_layer.begin_text_section();

    current_layer.set_font(&font, line_height - 2.0);
    current_layer.set_text_cursor(pdf_margin, pdf_height - (pdf_margin * 2.0) - hmm - logo_height_mm);
    current_layer.set_line_height(line_height);

    current_layer.write_text(text_line_1, &font);
    current_layer.add_line_break();
    current_layer.write_text(text_line_2, &font);
    current_layer.add_line_break();
    current_layer.write_text(text_line_3, &font);

    current_layer.end_text_section();

    let barcode_image = PdfImage::from_dynamic_image(&image::load_from_memory(image_bytes)?);
    // We want the barcode width to fit.
    let original_width = barcode_image.image.width.into_pt(DPI);
    let new_width: Pt = (pdf_width - (pdf_margin * 2.0)).into();
    let width_scale = new_width / original_width;
    let barcode_height: Pt = barcode_image.image.height.into_pt(DPI) * width_scale;
    let barcode_height_mm: Mm = From::from(barcode_height);
    let translate_y = pdf_height - logo_height_mm - hmm - (pdf_margin * 3.0);
    // translate x, translate y, rotate, scale x, scale y
    // rotations and translations are always in relation to the lower left corner
    barcode_image.add_to_layer(
        current_layer,
        printpdf::ImageTransform {
            translate_x: Some(pdf_margin),
            translate_y: Some((barcode_height_mm - (translate_y / 2.0)) * -1.0),
            rotate: None,
            scale_x: Some(width_scale),
            scale_y: Some(width_scale),
            dpi: Some(DPI),
        },
    );

    // Save the PDF
    let mut bw = BufWriter::new(Vec::new());

    doc.save(&mut bw)?;

    Ok(bw.into_inner()?)
}

/// A request to print labels.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct PrintRequest {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default)]
    pub quantity: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
}

impl SwagInventoryItem {
    /// Send the label to our printer.
    #[tracing::instrument]
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
            let parent_id = drive_client.files().create_folder(&drive_id, "", "swag").await?;

            let mut sw: NewSwagInventoryItem = From::from(self.clone());
            sw.expand(&drive_client, &drive_id, &parent_id).await?
        } else {
            self.barcode_pdf_label.trim().to_string()
        };

        let printer_url = format!("{}/zebra", company.printer_url);
        let client = reqwest::Client::new();
        let resp = client
            .post(&printer_url)
            .body(
                json!(PrintRequest {
                    url,
                    quantity: self.print_barcode_label_quantity,
                    content: String::new(),
                })
                .to_string(),
            )
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

    #[tracing::instrument]
    pub async fn get_item(&self, db: &Database) -> Option<SwagItem> {
        SwagItem::get_from_db(db, self.item.to_string()).await
    }

    #[tracing::instrument]
    pub async fn send_slack_notification_if_inventory_changed(
        &mut self,
        db: &Database,
        company: &Company,
        new: i32,
    ) -> Result<()> {
        let send_notification = self.current_stock != new;

        if send_notification {
            // Send a slack notification since it changed.
            let mut msg: FormattedMessage = self.clone().into();

            let item = self.get_item(db).await.unwrap();

            // Add our image as an accessory.
            let accessory = MessageBlockAccessory {
                accessory_type: MessageType::Image,
                image_url: item.image.to_string(),
                alt_text: self.item.to_string(),
                text: None,
                action_id: Default::default(),
                value: Default::default(),
            };

            // Set our accessory.
            msg.attachments[0].blocks[0].accessory = Some(accessory);
            // Set our text.
            let mut t = msg.attachments[0].blocks[0].text.as_ref().unwrap().clone();
            t.text = format!(
                "*{}*\nstock changed from `{}` to `{}`",
                self.name, self.current_stock, new
            );
            msg.attachments[0].blocks[0].text = Some(t);

            if self.current_stock > new {
                msg.attachments[0].color = crate::colors::Colors::Yellow.to_string();
            } else {
                // We increased in stock, show it as Green.
                msg.attachments[0].color = crate::colors::Colors::Green.to_string();
            }

            msg.channel = company.slack_channel_swag.to_string();

            company.post_to_slack_channel(db, &msg).await?;
        }

        // Set the new count.
        self.current_stock = new;

        Ok(())
    }
}

/// Sync swag inventory items from Airtable.
pub async fn refresh_swag_inventory_items(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_swag.is_empty() {
        // Return early.
        return Ok(());
    }

    // Initialize the Google Drive client.
    let drive_client = company.authenticate_google_drive(db).await?;

    // Figure out where our directory is.
    // It should be in the shared drive : "Automated Documents"/"rfds"
    let shared_drive = drive_client.drives().get_by_name("Automated Documents").await?;
    let drive_id = shared_drive.id.to_string();

    // Get the directory by the name.
    let parent_id = drive_client.files().create_folder(&drive_id, "", "swag").await?;

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagInventoryItem>> = company
        .authenticate_airtable(&company.airtable_base_id_swag)
        .list_records(&SwagInventoryItem::airtable_table(), "Grid view", vec![])
        .await?;
    for inventory_item_record in results {
        let mut inventory_item: NewSwagInventoryItem = inventory_item_record.fields.into();
        inventory_item.expand(&drive_client, &drive_id, &parent_id).await?;
        inventory_item.cio_company_id = company.id;

        // TODO: send a slack notification for a new item (?)

        let mut db_inventory_item = inventory_item.upsert_in_db(db).await?;
        db_inventory_item.airtable_record_id = inventory_item_record.id.to_string();
        db_inventory_item.update(db).await?;
    }

    SwagInventoryItems::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
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
#[diesel(table_name = barcode_scans)]
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
    #[tracing::instrument]
    async fn update_airtable_record(&mut self, _record: BarcodeScan) -> Result<()> {
        Ok(())
    }
}

impl BarcodeScan {
    // Takes a scanned barcode and updates the inventory count for the item
    // as well as adds the scan to the barcodes_scan table for tracking.
    #[tracing::instrument]
    pub async fn scan(b: String) -> Result<()> {
        let time = Utc::now();

        // Make sure the barcode is formatted correctly.
        let barcode = b.trim().to_uppercase().to_string();

        // Initialize the database connection.
        let db = Database::new().await;

        // Firstly, let's make sure we have the barcode in the database.
        match swag_inventory_items::dsl::swag_inventory_items
            .filter(swag_inventory_items::dsl::barcode.eq(barcode.to_string()))
            .first_async::<SwagInventoryItem>(&db.pool())
            .await
        {
            Ok(mut swag_inventory_item) => {
                // We found the matching inventory item!
                // Now let's subtract 1 from the current inventory and update it
                // in the database.
                swag_inventory_item.current_stock -= 1;
                // Update the database.
                swag_inventory_item.update(&db).await?;
                info!(
                    "subtracted one from {} stock, we now have {}",
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
                new_barcode_scan.upsert(&db).await?;
            }
            Err(e) => bail!("could not find inventory item with barcode {}: {}", barcode, e),
        }

        Ok(())
    }
}

pub async fn refresh_barcode_scans(db: &Database, company: &Company) -> Result<()> {
    if company.airtable_base_id_swag.is_empty() {
        // Return early.
        return Ok(());
    }

    BarcodeScans::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}
