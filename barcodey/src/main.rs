use std::env;

use sentry::IntoDsn;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Initialize sentry.
    let sentry_dsn = env::var("BARCODEY_SENTRY_DSN").unwrap_or_default();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: sentry_dsn.into_dsn().unwrap(),

        release: Some(env::var("GIT_HASH").unwrap_or_default().into()),
        environment: Some(env::var("SENTRY_ENV").unwrap_or_else(|_| "development".to_string()).into()),
        ..Default::default()
    });

    Ok(())
}
