use async_trait::async_trait;
use barcoders::generators::image::*;
use barcoders::generators::svg::*;
use barcoders::sym::code39::*;
use chrono::{DateTime, Utc};
use google_drive::GoogleDrive;
use image::{DynamicImage, ImageFormat};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat};
use macros::db;
use reqwest::StatusCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_BARCODE_SCANS_TABLE, AIRTABLE_SWAG_INVENTORY_ITEMS_TABLE, AIRTABLE_SWAG_ITEMS_TABLE};
use crate::companies::Company;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{barcode_scans, swag_inventory_items, swag_items};

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
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
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
pub async fn refresh_swag_items() {
    let db = Database::new();

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagItem>> = oxide
        .authenticate_airtable(&oxide.airtable_base_id_swag)
        .list_records(&SwagItem::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
    for item_record in results {
        let mut item: NewSwagItem = item_record.fields.into();
        item.cio_company_id = oxide.id;

        let mut db_item = item.upsert_in_db(&db);
        db_item.airtable_record_id = item_record.id.to_string();
        db_item.update(&db).await;
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

    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
    pub barcode_png: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
    pub barcode_svg: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "airtable_api::attachment_format_as_string::deserialize")]
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
            println!("len too long {} {}, needs to be {} or under", barcode, barcode.len(), max_barcode_len);
        }

        self.barcode = barcode;
    }

    pub async fn generate_barcode_images(&mut self, drive_client: &GoogleDrive) {
        // Generate the barcode.
        // "Name" is automatically generated by Airtable from the item and the size.
        if !self.name.is_empty() {
            let bucket = "oxide_automated_documents";
            // Generate the barcode svg and png.
            let barcode = Code39::new(&self.barcode).unwrap();
            let png = Image::png(45); // You must specify the height in pixels.
            let encoded = barcode.encode();

            // Image generators return a Result<Vec<u8>, barcoders::error::Error) of encoded bytes.
            let png_bytes = png.generate(&encoded[..]).unwrap();
            let mut file_name = format!("swag/barcodes/png/{}.png", self.name.replace('/', ""));

            // Create or update the files in the google_drive.
            let png_file = drive_client.upload_to_cloud_storage(bucket, &file_name, "image/png", &png_bytes, true).await.unwrap();
            self.barcode_png = png_file.media_link.to_string();

            // Now do the SVG.
            let svg = SVG::new(200); // You must specify the height in pixels.
            let svg_data: String = svg.generate(&encoded).unwrap();
            let svg_bytes = svg_data.as_bytes();

            file_name = format!("swag/barcodes/svg/{}.svg", self.name.replace('/', ""));

            // Create or update the files in the google_drive.
            let svg_file = drive_client.upload_to_cloud_storage(bucket, &file_name, "image/svg+xml", &svg_bytes, true).await.unwrap();
            self.barcode_svg = svg_file.media_link.to_string();

            // Generate the barcode label.
            let label_bytes = self.generate_pdf_barcode_label(&png_bytes);
            file_name = format!("swag/barcodes/pdf/{} - Barcode Label.pdf", self.name.replace('/', ""));
            // Create or update the files in the google_drive.
            let label_file = drive_client.upload_to_cloud_storage(bucket, &file_name, "application/pdf", &label_bytes, true).await.unwrap();
            self.barcode_pdf_label = label_file.media_link;
        }
    }

    // Get the bytes for a pdf barcode label.
    pub fn generate_pdf_barcode_label(&self, png_bytes: &[u8]) -> Vec<u8> {
        let pdf_width = 3.0 * 72.0;
        let pdf_height = 2.0 * 72.0;
        let pdf_margin = 5.0;
        let font_size = 9.0;
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), (font_size / 1.25).into()]),
                Operation::new("TL", vec![(font_size * 1.25).into()]),
                Operation::new("Td", vec![pdf_margin.into(), (font_size * 0.9 * 3.0).into()]),
                Operation::new("Tj", vec![Object::string_literal(self.barcode.to_string())]),
                Operation::new("Tf", vec!["F1".into(), font_size.into()]),
                Operation::new("'", vec![Object::string_literal(self.item.to_string())]),
                Operation::new("'", vec![Object::string_literal(format!("Size: {}", self.size))]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => resources_id,
            // This should be (4 in x 6 in) for the rollo printer.
            // You get `pts` by (inches * 72).
            "MediaBox" => vec![0.into(), 0.into(),pdf_width.into(), pdf_height.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let logo_bytes = include_bytes!("oxide_logo.png");
        let (mut doc, logo_stream, mut logo_info) = image_to_pdf_object(doc, logo_bytes);
        // We want the logo width to fit.
        let original_width = logo_info.width;
        logo_info.width = pdf_width - (pdf_margin * 2.0);
        logo_info.height *= logo_info.width / original_width;
        let position = ((pdf_width - logo_info.width) / 2.0, pdf_height - logo_info.height - pdf_margin);
        // Center the logo at the top of the pdf.
        doc.insert_image(page_id, logo_stream, position, (logo_info.width, logo_info.height)).unwrap();

        let (mut doc, img_stream, info) = image_to_pdf_object(doc, png_bytes);
        // We want the barcode width to fit.
        // This will center it automatically.
        let position = ((pdf_width - info.width) / 2.0, pdf_height - info.height - logo_info.height - (pdf_margin * 2.0));
        // Center the barcode at the top of the pdf.
        doc.insert_image(page_id, img_stream, position, (info.width, info.height)).unwrap();

        doc.compress();

        // Save the PDF
        let mut buffer = Vec::new();
        doc.save_to(&mut buffer).unwrap();
        buffer
    }

    pub async fn expand(&mut self, drive_client: &GoogleDrive) {
        self.generate_barcode();
        self.generate_barcode_images(drive_client).await;
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

impl SwagInventoryItem {
    /// Send the label to our printer.
    pub async fn print_label(&self, company: &Company) {
        if self.barcode_pdf_label.trim().is_empty() {
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
                panic!("[print]: status_code: {}, body: {}", s, resp.text().await.unwrap());
            }
        };
    }
}

pub fn image_to_pdf_object(mut doc: Document, png_bytes: &[u8]) -> (Document, Stream, crate::png::PngInfo) {
    // Insert our barcode image.
    let info = crate::png::get_info(png_bytes);

    let bytes = if info.interlace || info.color_type >= 4 {
        let img = image::load_from_memory(png_bytes).unwrap();
        let mut result = Vec::new();

        match info.color_type {
            4 => match info.depth {
                8 => DynamicImage::ImageLuma8(img.into_luma8()),
                16 => DynamicImage::ImageLuma16(img.into_luma16()),
                _ => panic!(""),
            },
            6 => match info.depth {
                8 => DynamicImage::ImageRgb8(img.into_rgb8()),
                16 => DynamicImage::ImageRgb16(img.into_rgb16()),
                _ => panic!(""),
            },
            _ => img,
        }
        .write_to(&mut result, ImageFormat::Png)
        .unwrap();
        result
    } else {
        png_bytes.into()
    };

    let colors = if let 0 | 3 | 4 = info.color_type { 1 } else { 3 };

    let idat = crate::png::get_idat(&bytes[..]);

    let cs: Object = match info.color_type {
        0 | 2 | 4 | 6 => {
            if let Some(ref raw) = info.icc {
                let icc_id = doc.add_object(Stream::new(
                    dictionary! {
                        "N" => colors,
                        "Alternate" => if let 0 | 4 = info.color_type { "DeviceGray" } else { "DeviceRGB" },
                        "Length" => raw.len() as u32,
                        "Filter" => "FlateDecode"
                    },
                    raw.to_vec(),
                ));
                vec!["ICCBased".into(), icc_id.into()].into()
            } else {
                if let 0 | 4 = info.color_type { "DeviceGray" } else { "DeviceRGB" }.into()
            }
        }

        3 => {
            let palette = info.clone().palette.unwrap();
            vec!["Indexed".into(), "DeviceRGB".into(), (palette.1 - 1).into(), Object::String(palette.0, StringFormat::Hexadecimal)].into()
        }

        _ => panic!("unexpected color type found: {}", info.color_type),
    };

    let img_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Filter" => "FlateDecode",
            "BitsPerComponent" => info.depth,
            "Length" => idat.len() as u32,
            "Width" => info.width as u32,
            "Height" => info.height as u32,
            "DecodeParms" => dictionary!{
                "BitsPerComponent" => info.depth,
                "Predictor" => 15,
                "Columns" => info.width as u32,
                "Colors" => colors
            },
            "ColorSpace" => cs,
        },
        idat,
    );

    (doc, img_stream, info)
}

/// Sync swag inventory items from Airtable.
pub async fn refresh_swag_inventory_items() {
    let db = Database::new();

    // Get the company id for Oxide.
    // TODO: split this out per company.
    let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

    // Get gsuite token.
    let token = oxide.authenticate_google(&db).await;

    // Initialize the Google Drive client.
    let drive_client = GoogleDrive::new(token);

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SwagInventoryItem>> = oxide
        .authenticate_airtable(&oxide.airtable_base_id_swag)
        .list_records(&SwagInventoryItem::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
    for inventory_item_record in results {
        let mut inventory_item: NewSwagInventoryItem = inventory_item_record.fields.into();
        inventory_item.expand(&drive_client).await;
        inventory_item.cio_company_id = oxide.id;

        let mut db_inventory_item = inventory_item.upsert_in_db(&db);
        db_inventory_item.airtable_record_id = inventory_item_record.id.to_string();
        db_inventory_item.update(&db).await;
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
                println!("Subtracted one from {} stock, we now have {}", swag_inventory_item.name, swag_inventory_item.current_stock);

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
            Err(e) => println!("could not find inventory item with barcode {}: {}", barcode, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::companies::Company;
    use crate::db::Database;
    use crate::swag_inventory::{refresh_swag_inventory_items, refresh_swag_items, BarcodeScans};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_swag_items() {
        refresh_swag_items().await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_swag_inventory_items() {
        refresh_swag_inventory_items().await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_refresh_barcode_scans() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        BarcodeScans::get_from_db(&db).update_airtable(&db, oxide.id).await;
    }
}
