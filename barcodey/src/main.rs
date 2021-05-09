use std::env;
use std::process::Command;

use cio_api::swag_inventory::BarcodeScan;
use sentry::IntoDsn;
use std::io::stdin;
use termion::input::TermRead;

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

    let stdin = stdin();

    // Detecting keydown events.
    // The barcode scanner works like a keyboard.
    // Trying to hijack it over HID failed.
    let mut chars: Vec<char> = Default::default();
    for c in stdin.keys() {
        match c.unwrap() {
            termion::event::Key::Char(ch) => {
                if ch == '\n' {
                    // If its a new line character we are at the end of a code.
                    // Combine all the characters together into a string.
                    let barcode: String = chars.into_iter().collect();
                    println!("Got barcode: {}", barcode);

                    // We got a barcode scan, lets add it to our database.
                    BarcodeScan::scan(barcode.trim().to_string()).await;

                    // Clear out the vector so we can scan again.
                    chars = vec![];
                    continue;
                }

                // We have a character and its not a new line.
                // Let's add it to our vector.
                chars.push(ch);
            }
            _ => (),
        }
    }

    Ok(())
}
