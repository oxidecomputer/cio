use anyhow::Result;
use cio_api::{
    app_config::AppConfig,
    applicant_uploads::UploadTokenStore,
    companies::{Company, Companys},
    configs::get_configs_from_repo,
    db::Database,
};
use std::sync::{Arc, RwLock};

use crate::sagas::{create_registry, Saga};

#[derive(Clone, Debug)]
pub struct ServerContext {
    pub sec: Arc<steno::SecClient>,
    pub exec_registry: Arc<steno::ActionRegistry<Saga>>,
    pub app: Context,
}

impl ServerContext {
    pub async fn new(company_id: i32, logger: slog::Logger) -> Result<ServerContext> {
        let context = Context::new(company_id).await?;

        Ok(Self {
            sec: Arc::new(steno::sec(logger, Arc::new(context.db.clone()))),
            exec_registry: Arc::new(create_registry()),
            app: context,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Context {
    pub app_config: Arc<RwLock<AppConfig>>,
    pub db: Database,
    pub company: Company,
    pub upload_token_store: UploadTokenStore,
}

impl Context {
    /**
     * Return a new Context.
     */
    pub async fn new(company_id: i32) -> Result<Context> {
        let db = Database::new().await;

        let company = Companys::get_from_db(&db, company_id)
            .await?
            .0
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Failed to find company record"))?;
        let github = company.authenticate_github()?;
        let configs = get_configs_from_repo(&github, &company).await?;

        // Create the context.
        Ok(Context {
            app_config: Arc::new(RwLock::new(configs.app_config)),
            db: db.clone(),
            company,
            upload_token_store: UploadTokenStore::new(db, chrono::Duration::minutes(10)),
        })
    }
}
