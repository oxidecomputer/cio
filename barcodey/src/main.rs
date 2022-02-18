use std::{env, process::Command};

use cio_api::swag_inventory::BarcodeScan;
use hidapi::HidApi;
use log::info;
use sentry::IntoDsn;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Initialize our logger.
    let mut log_builder = pretty_env_logger::formatted_builder();
    log_builder.parse_filters("info");

    let logger = sentry_log::SentryLogger::with_dest(log_builder.build());

    log::set_boxed_logger(Box::new(logger)).unwrap();

    log::set_max_level(log::LevelFilter::Info);

    // Try to get the current git hash.
    let git_hash = if let Ok(gh) = env::var("GIT_HASH") {
        gh
    } else {
        // Try to shell out.
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .expect("failed to execute process");
        let o = std::str::from_utf8(&output.stdout).unwrap();
        o[0..8].to_string()
    };
    info!("git hash: {}", git_hash);

    // Initialize sentry.
    // In addition to all the sentry env variables, you will also need to set
    //  - CIO_DATABASE_URL
    let sentry_dsn = env::var("BARCODEY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(git_hash.into()),
        environment: Some(
            env::var("SENTRY_ENV")
                .unwrap_or_else(|_| "development".to_string())
                .into(),
        ),
        default_integrations: true,
        ..sentry::ClientOptions::default()
    });

    let api = HidApi::new().expect("Failed to create API instance");
    let mut vendor_id: u16 = u16::MIN;
    let mut product_id: u16 = u16::MIN;

    // Iterate over our devices.
    // Try and find the barcode scanner.
    let search = "";
    for device in api.device_list() {
        info!(
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

        if device.product_string().unwrap_or_default() == search
            && device.product_id() == u16::from_str_radix("011a", 16).unwrap()
        {
            // We found our device.
            vendor_id = device.vendor_id();
            product_id = device.product_id();
        }
    }

    if vendor_id == u16::MIN && product_id == u16::MIN {
        return Err("could not find barcode scanner in HID devices".to_string());
    }

    // Open the scanner device and listen for events to read.
    let scanner = api.open(vendor_id, product_id).expect("Failed to open device");
    info!(
        "listening for events from (vendor ID: {} {:04x}) (product ID: {} {:04x}) in a loop...",
        vendor_id, vendor_id, product_id, product_id
    );

    // This stores our set of characters.
    // When a return character is observed we will flush this.
    let mut chars: Vec<char> = Default::default();
    loop {
        let mut buf = [0u8; 256];
        let res = scanner.read(&mut buf[..]).unwrap();

        // We know these come in as keycodes so:
        // - The first byte is the modifier (we know its always uppercase so let's ignore.
        // - The second byte we will skip as well.
        // - The last 6 bytes are the keycode. We want to collect those.
        let mut key: u8 = Default::default();
        for (i, u) in buf[..res].iter().enumerate() {
            // We skip the first 2.
            if i == 2 {
                key = *u;
            }
        }

        // Now let's match the keycode to our chars.
        let c = if keycode::KeyMap::from(keycode::KeyMappingId::UsA).usb == key as u16 {
            'A'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsB).usb == key as u16 {
            'B'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsC).usb == key as u16 {
            'C'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsD).usb == key as u16 {
            'D'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsE).usb == key as u16 {
            'E'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsF).usb == key as u16 {
            'F'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsG).usb == key as u16 {
            'G'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsH).usb == key as u16 {
            'H'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsI).usb == key as u16 {
            'I'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsJ).usb == key as u16 {
            'J'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsK).usb == key as u16 {
            'K'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsL).usb == key as u16 {
            'L'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsM).usb == key as u16 {
            'M'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsN).usb == key as u16 {
            'N'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsO).usb == key as u16 {
            'O'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsP).usb == key as u16 {
            'P'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsQ).usb == key as u16 {
            'Q'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsR).usb == key as u16 {
            'R'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsS).usb == key as u16 {
            'S'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsT).usb == key as u16 {
            'T'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsU).usb == key as u16 {
            'U'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsV).usb == key as u16 {
            'V'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsW).usb == key as u16 {
            'W'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsX).usb == key as u16 {
            'X'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsY).usb == key as u16 {
            'Y'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::UsZ).usb == key as u16 {
            'Z'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit0).usb == key as u16 {
            '0'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit1).usb == key as u16 {
            '1'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit2).usb == key as u16 {
            '2'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit3).usb == key as u16 {
            '3'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit4).usb == key as u16 {
            '4'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit5).usb == key as u16 {
            '5'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit6).usb == key as u16 {
            '6'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit7).usb == key as u16 {
            '7'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit8).usb == key as u16 {
            '8'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Digit9).usb == key as u16 {
            '9'
        } else if keycode::KeyMap::from(keycode::KeyMappingId::Enter).usb == key as u16 {
            '\n'
        } else {
            Default::default()
        };

        if c == '\n' {
            // If its a new line character we are at the end of a code.
            // Combine all the characters together into a string.
            let barcode: String = chars.into_iter().collect();
            info!("got barcode: {}", barcode);

            // We got a barcode scan, lets add it to our database.
            BarcodeScan::scan(barcode.trim().to_string()).await.unwrap();

            // Clear out the vector so we can scan again.
            chars = vec![];
            // Continue the loop.
            continue;
        } else if c == char::default() {
            // We have the default byte.
            continue;
        }

        // We have a character and its not a new line.
        // Let's add it to our vector.
        chars.push(c);
    }
}
