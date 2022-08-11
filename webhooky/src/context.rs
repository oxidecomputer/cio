use anyhow::Result;
use cio_api::{applicant_uploads::UploadTokenStore, companies::{Company, Companys}, configs::get_configs_from_repo, app_config::AppConfig, db::Database};
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct Context {
    pub app_config: Arc<RwLock<AppConfig>>,
    pub db: Database,
    pub company: Company,
    pub sec: Arc<steno::SecClient>,
    pub schema: serde_json::Value,
    pub upload_token_store: UploadTokenStore,
}

impl Context {
    /**
    * Return a new Context.
    */
    pub async fn new(company_id: i32, schema: serde_json::Value, logger: slog::Logger) -> Result<Context> {
        let db = Database::new().await;
        let sec = steno::sec(logger, Arc::new(db.clone()));

        let company = Companys::get_from_db(&db, company_id).await?.0.pop().ok_or_else(|| anyhow::anyhow!("Failed to find company record"))?;
        let github = company.authenticate_github()?;
        let configs = get_configs_from_repo(&github, &company).await?;

        // Create the context.
        Ok(Context {
            app_config: Arc::new(RwLock::new(configs.app_config)),
            db: db.clone(),
            company,
            sec: Arc::new(sec),
            schema,
            upload_token_store: UploadTokenStore::new(db, chrono::Duration::minutes(10)),
        })
    }
}