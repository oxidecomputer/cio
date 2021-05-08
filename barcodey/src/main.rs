use std::env;

use cio_api::swag_inventory::BarcodeScan;
use hidapi::HidApi;
use sentry::IntoDsn;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Try to get the current git hash.
    let git_hash = if let Ok(gh) = env::var("GIT_HASH") {
        gh
    } else {
        // Try to shell out.
        let output = Command::new("git").arg("rev-parse").arg("HEAD").output().expect("failed to execute process");
        let o = std::str::from_utf8(&output.stdout).unwrap();
        o[0..8].to_string()
    };

    println!("git hash: {}", git_hash);

    // Initialize sentry.
    // In addition to all the sentry env variables, you will also need to set
    //  - CIO_DATABASE_URL
    //  - AIRTABLE_API_KEY
    let sentry_dsn = env::var("BARCODEY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(git_hash.into()),
        environment: Some(env::var("SENTRY_ENV").unwrap_or_else(|_| "development".to_string()).into()),
        ..Default::default()
    });

    let api = HidApi::new().expect("Failed to create API instance");
    let vendor_id: u16 = u16::MIN;
    let product_id: u16 = u16::MIN;

    // Iterate over our devices.
    // Try and find the barcode scanner.
    for device in api.device_list() {
        println!(
            "VID: {:04x}, PID: {:04x}, Serial: {}, Product name: {}",
            device.vendor_id(),
            device.product_id(),
            match device.serial_number() {
                Some(s) => s,
                _ => "<COULD NOT FETCH>",
            },
            match device.product_string() {
                Some(s) => s,
                _ => "<COULD NOT FETCH>",
            }
        );
    }

    if vendor_id == u16::MIN && product_id == u16::MIN {
        return Err("could not find barcode scanner in HID devices".to_string());
    }

    // Open the scanner device and listen for events to read.
    let scanner = api.open(vendor_id, product_id).expect("Failed to open device");
    println!("Listening for events from (vendor ID: {}) (product ID: {}) in a loop...", vendor_id, product_id);

    loop {
        let mut buf = [0u8; 256];
        let res = scanner.read(&mut buf[..]).unwrap();

        let mut data_string = String::new();

        for u in &buf[..res] {
            data_string.push_str(&(u.to_string() + "\t"));
        }

        println!("{}", data_string);

        // We got a barcode scan, lets add it to our database.
        BarcodeScan::scan(data_string.trim().to_string()).await;
    }
}
