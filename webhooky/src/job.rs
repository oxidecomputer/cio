use crate::context::Context;
use anyhow::Result;

pub async fn run_job_cmd(cmd: crate::core::SubCommand, context: Context) -> Result<()> {
    match cmd {
        crate::core::SubCommand::SendRFDChangelog(_) => {
            let Context { db, company, .. } = context;
            cio_api::rfd::send_rfd_changelog(&db, &company).await?;
        }
        crate::core::SubCommand::SyncAnalytics(_) => {
            let Context { db, company, .. } = context;
            cio_api::analytics::refresh_analytics(&db, &company).await?;
        }
        crate::core::SubCommand::SyncAPITokens(_) => {
            let Context { db, company, .. } = context;
            cio_api::api_tokens::refresh_api_tokens(&db, &company).await?;
        }
        crate::core::SubCommand::SyncApplications(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;

            // Do the new applicants.
            let app_config = app_config.read().unwrap().clone();
            cio_api::applicants::refresh_new_applicants_and_reviews(&db, &company, &app_config).await?;
            cio_api::applicant_reviews::refresh_reviews(&db, &company).await?;

            // Refresh DocuSign for the applicants.
            cio_api::applicants::refresh_docusign_for_applicants(&db, &company, &app_config).await?;
        }
        crate::core::SubCommand::SyncAssetInventory(_) => {
            let Context { db, company, .. } = context;
            cio_api::asset_inventory::refresh_asset_items(&db, &company).await?;
        }
        crate::core::SubCommand::SyncCompanies(_) => {
            let Context { db, .. } = context;
            cio_api::companies::refresh_companies(&db).await?;
        }
        crate::core::SubCommand::SyncConfigs(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;
            let config = app_config.read().unwrap().clone();
            cio_api::configs::refresh_db_configs_and_airtable(&db, &company, &config).await?;
        }
        crate::core::SubCommand::SyncFinance(_) => {
            let Context {
                app_config,
                db,
                company,
                ..
            } = context;
            let app_config = app_config.read().unwrap().clone();
            cio_api::finance::refresh_all_finance(&db, &company, &app_config.finance).await?;
        }
        crate::core::SubCommand::SyncFunctions(_) => {
            let Context { db, company, .. } = context;
            cio_api::functions::refresh_functions(&db, &company).await?;
        }
        crate::core::SubCommand::SyncHuddles(_) => {
            let Context { db, company, .. } = context;
            cio_api::huddles::sync_changes_to_google_events(&db, &company).await?;
            cio_api::huddles::sync_huddles(&db, &company).await?;
            cio_api::huddles::send_huddle_reminders(&db, &company).await?;
            cio_api::huddles::sync_huddle_meeting_notes(&company).await?;
        }
        crate::core::SubCommand::SyncInterviews(_) => {
            let Context { db, company, .. } = context;
            cio_api::interviews::refresh_interviews(&db, &company).await?;
            cio_api::interviews::compile_packets(&db, &company).await?;
        }
        crate::core::SubCommand::SyncJournalClubs(_) => {
            let Context { db, company, .. } = context;
            cio_api::journal_clubs::refresh_db_journal_club_meetings(&db, &company).await?;
        }
        crate::core::SubCommand::SyncMailingLists(_) => {
            if std::env::var("MAILERLITE_ENABLED")
                .map(|v| v == "true")
                .unwrap_or(false)
            {
                let Context { db, .. } = context;

                crate::mailing_lists::sync_pending_mailing_list_subscribers(&db).await?;
                crate::mailing_lists::sync_pending_wait_list_subscribers(&db).await?;
            }
        }
        crate::core::SubCommand::SyncRecordedMeetings(_) => {
            let Context { db, company, .. } = context;
            cio_api::recorded_meetings::refresh_zoom_recorded_meetings(&db, &company).await?;
            cio_api::recorded_meetings::refresh_google_recorded_meetings(&db, &company).await?;
        }
        crate::core::SubCommand::SyncRepos(_) => {
            let Context { db, company, .. } = context;
            let sync_result = cio_api::repos::sync_all_repo_settings(&db, &company).await;
            let refresh_result = cio_api::repos::refresh_db_github_repos(&db, &company).await;

            if let Err(ref e) = sync_result {
                log::error!("Failed syncing repo settings {:?}", e);
            }

            if let Err(ref e) = refresh_result {
                log::error!("Failed refreshing GitHub db repos {:?}", e);
            }

            sync_result?;
            refresh_result?;
        }
        crate::core::SubCommand::SyncRFDs(_) => {
            let Context { db, company, .. } = &context;
            crate::handlers_rfd::refresh_db_rfds(&context).await?;
            cio_api::rfd::drive::cleanup_rfd_pdfs(db, company).await?;
        }
        crate::core::SubCommand::SyncOther(_) => {
            let Context { company, .. } = context;
            cio_api::tailscale::cleanup_old_tailscale_devices(&company).await?;
            cio_api::tailscale::cleanup_old_tailscale_cloudflare_dns(&company).await?;
            cio_api::customers::sync_customer_meeting_notes(&company).await?;
        }
        crate::core::SubCommand::SyncShipments(_) => {
            let Context { db, company, .. } = context;
            let inbound_result = cio_api::shipments::refresh_inbound_shipments(&db, &company).await;
            let outbound_result = cio_api::shipments::refresh_outbound_shipments(&db, &company).await;

            if let Err(ref e) = inbound_result {
                log::error!("Failed to refresh inbound shipments {:?}", e);
            }

            if let Err(ref e) = outbound_result {
                log::error!("Failed to refresh outbound shipments {:?}", e);
            }

            inbound_result?;
            outbound_result?;
        }
        crate::core::SubCommand::SyncShorturls(_) => {
            let Context { db, company, .. } = context;
            cio_api::shorturls::refresh_shorturls(&db, &company).await?;
        }
        crate::core::SubCommand::SyncSwagInventory(_) => {
            let Context { db, company, .. } = context;
            cio_api::swag_inventory::refresh_swag_items(&db, &company).await?;
            cio_api::swag_inventory::refresh_swag_inventory_items(&db, &company).await?;
            cio_api::swag_inventory::refresh_barcode_scans(&db, &company).await?;
        }
        crate::core::SubCommand::SyncTravel(_) => {
            let Context { db, company, .. } = context;
            cio_api::travel::refresh_trip_actions(&db, &company).await?;
        }
        crate::core::SubCommand::SyncZoho(_) => {
            let Context { db, company, .. } = context;
            cio_api::zoho::refresh_leads(&db, &company).await?;
        }
        other => anyhow::bail!("Non-job subcommand passed to job runner {:?}", other),
    }

    Ok(())
}
