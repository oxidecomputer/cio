use std::{collections::HashMap, env, fs::File};

use anyhow::{bail, Result};
use async_bb8_diesel::AsyncRunQueryDsl;
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use log::{info, warn};
use macros::db;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slack_chat_api::{
    FormattedMessage, MessageAttachment, MessageBlock, MessageBlockText, MessageBlockType, MessageType,
};

use crate::{
    airtable::{
        AIRTABLE_ACCOUNTS_PAYABLE_TABLE, AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE, AIRTABLE_EXPENSED_ITEMS_TABLE,
        AIRTABLE_SOFTWARE_VENDORS_TABLE,
    },
    app_config::FinanceConfig,
    companies::Company,
    configs::{Group, User},
    core::UpdateAirtableRecord,
    db::Database,
    providers::ProviderReadOps,
    schema::{accounts_payables, credit_card_transactions, expensed_items, software_vendors, users},
};

#[db {
    new_struct_name = "SoftwareVendor",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_SOFTWARE_VENDORS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = software_vendors)]
pub struct NewSoftwareVendor {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub website: String,
    #[serde(default)]
    pub has_okta_integration: bool,
    #[serde(default)]
    pub used_purely_for_api: bool,
    #[serde(default)]
    pub pay_as_you_go: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pay_as_you_go_pricing_description: String,
    #[serde(default)]
    pub software_licenses: bool,
    #[serde(default)]
    pub cost_per_user_per_month: f32,
    #[serde(default)]
    pub users: i32,
    #[serde(default)]
    pub flat_cost_per_month: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub total_cost_per_month: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// This is linked to another table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_transactions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_accounts_payable: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_expensed_items: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// This is only used for serialize
fn is_zero(num: &f32) -> bool {
    *num == 0.0
}

/// Implement updating the Airtable record for a SoftwareVendor.
#[async_trait]
impl UpdateAirtableRecord<SoftwareVendor> for SoftwareVendor {
    async fn update_airtable_record(&mut self, record: SoftwareVendor) -> Result<()> {
        // This is a function so we can't change it through the API.
        self.total_cost_per_month = 0.0;
        // Keep this the same, we update it from the transactions.
        self.link_to_transactions = record.link_to_transactions;
        // Keep this the same, we update it from the accounts payable.
        self.link_to_accounts_payable = record.link_to_accounts_payable;
        // Keep this the same, we update it from the expensed items.
        self.link_to_expensed_items = record.link_to_expensed_items;

        Ok(())
    }
}

/// Convert the vendor into a Slack message.
impl From<NewSoftwareVendor> for FormattedMessage {
    fn from(item: NewSoftwareVendor) -> Self {
        FormattedMessage {
            channel: Default::default(),
            blocks: Default::default(),
            attachments: vec![MessageAttachment {
                color: Default::default(),
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
                        block_type: MessageBlockType::Header,
                        text: Some(MessageBlockText {
                            text_type: MessageType::PlainText,
                            text: item.name.to_string(),
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
                            text: format!("Vendors | {} | {} | _just now_", item.category, item.status),
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

impl From<SoftwareVendor> for FormattedMessage {
    fn from(item: SoftwareVendor) -> Self {
        let new: NewSoftwareVendor = item.into();
        new.into()
    }
}

impl NewSoftwareVendor {
    pub async fn send_slack_notification_if_price_changed(
        &mut self,
        db: &Database,
        company: &Company,
        new: i32,
        new_cost_per_user: f32,
    ) -> Result<()> {
        if self.cost_per_user_per_month == 0.0 {
            // Return early we don't care.
            return Ok(());
        }

        let send_notification = self.users != new || (self.cost_per_user_per_month - new_cost_per_user).abs() > 0.05;

        if send_notification {
            // Send a slack notification since it changed.
            let mut msg: FormattedMessage = self.clone().into();

            // Add text.
            let text = MessageBlock {
                block_type: MessageBlockType::Section,
                text: Some(MessageBlockText {
                    text_type: MessageType::Markdown,
                    text: format!(
                        "price changed from `{}` users @ `${}` to `{}` users @ `${}`, total: `${}`",
                        self.users,
                        self.cost_per_user_per_month,
                        new,
                        new_cost_per_user,
                        new as f32 * new_cost_per_user
                    ),
                }),
                elements: Default::default(),
                accessory: Default::default(),
                block_id: Default::default(),
                fields: Default::default(),
            };

            // Set our accessory.
            msg.attachments[0].blocks.insert(1, text);

            if self.users < new || self.cost_per_user_per_month < new_cost_per_user {
                msg.attachments[0].color = crate::colors::Colors::Blue.to_string();
            } else {
                // We decreased in price.
                msg.attachments[0].color = crate::colors::Colors::Green.to_string();
            }

            msg.channel = company.slack_channel_finance.to_string();

            company.post_to_slack_channel(db, &msg).await?;
        }

        // Set the new count.
        self.users = new;
        self.cost_per_user_per_month = new_cost_per_user;

        Ok(())
    }
}

/// Sync software vendors from Airtable.
pub async fn refresh_software_vendors(db: &Database, company: &Company) -> Result<()> {
    let gsuite = company.authenticate_google_admin(db).await?;

    let github = company.authenticate_github()?;

    let okta_auth = company.authenticate_okta();

    let slack_auth = company.authenticate_slack(db).await;

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SoftwareVendor>> = company
        .authenticate_airtable(&company.airtable_base_id_finance)
        .list_records(&SoftwareVendor::airtable_table(), "Grid view", vec![])
        .await?;
    for vendor_record in results {
        let mut vendor: NewSoftwareVendor = vendor_record.fields.into();

        let new_cost_per_user_per_month = vendor.cost_per_user_per_month;

        // Get the existing record if there is one.
        let existing = SoftwareVendor::get_from_db(db, company.id, vendor.name.to_string()).await;
        // Set the existing cost and number of users, since we want to know
        // via slack notification if it changed.
        vendor.cost_per_user_per_month = if let Some(ref ex) = existing {
            ex.cost_per_user_per_month
        } else {
            0.0
        };
        vendor.users = if let Some(ref ex) = existing { ex.users } else { 0 };

        // Set the company id.
        vendor.cio_company_id = company.id;

        let users = if vendor.name == "GitHub" {
            // Update the number of GitHub users in our org.
            let org = github.orgs().get(&company.github_org).await?;
            org.plan.unwrap().filled_seats as i32
        } else if vendor.name == "Okta" && okta_auth.is_some() {
            let okta = okta_auth.as_ref().unwrap();
            let users = okta.list_provider_users(company).await?;
            users.len() as i32
        } else if vendor.name == "Google Workspace" {
            let users = gsuite.list_provider_users(company).await?;
            users.len() as i32
        } else if vendor.name == "Slack" && slack_auth.is_ok() {
            let slack = slack_auth.as_ref().unwrap();
            let users = slack.billable_info().await?;
            let mut count = 0;
            for (_, user) in users {
                if user.billing_active {
                    count += 1;
                }
            }

            count
        } else if vendor.name == "Airtable"
            || vendor.name == "Ramp"
            || vendor.name == "Brex"
            || vendor.name == "Gusto"
            || vendor.name == "Expensify"
        {
            // Airtable, Brex, Gusto, Expensify are all the same number of users as
            // in all@.
            let group = Group::get_from_db(db, company.id, "all".to_string()).await.unwrap();
            let airtable_group = group.get_existing_airtable_record(db).await.unwrap();

            airtable_group.fields.members.len() as i32
        } else {
            vendor.users
        };

        // Send the slack notification if the number of users or cost changed.
        // This will also set the values for users and cost_per_user_per_month, so
        // do this before sending to the database.
        vendor
            .send_slack_notification_if_price_changed(db, company, users, new_cost_per_user_per_month)
            .await?;

        // Upsert the record in our database.
        let mut db_vendor = vendor.upsert_in_db(db).await?;

        if db_vendor.airtable_record_id.is_empty() {
            db_vendor.airtable_record_id = vendor_record.id;
        }

        // Update the cost per month.
        db_vendor.total_cost_per_month =
            (db_vendor.cost_per_user_per_month * db_vendor.users as f32) + db_vendor.flat_cost_per_month;

        db_vendor.update(db).await?;
    }

    SoftwareVendors::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

#[db {
    new_struct_name = "CreditCardTransaction",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "transaction_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = credit_card_transactions)]
pub struct NewCreditCardTransaction {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Id")]
    pub transaction_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub card_vendor: String,
    #[serde(default, alias = "Amount")]
    pub amount: f32,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub employee_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Last 4")]
    pub card_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "User")]
    pub merchant_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Merchant Name")]
    pub merchant_name: String,
    #[serde(default)]
    pub category_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Brex Category")]
    pub category_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Memo")]
    pub memo: String,
    #[serde(alias = "Swipe Time (UTC)")]
    pub time: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_array_of_strings::deserialize",
        serialize_with = "airtable_api::attachment_format_as_array_of_strings::serialize"
    )]
    pub receipts: Vec<String>,
    /// This is linked to another table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_vendor: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a CreditCardTransaction.
#[async_trait]
impl UpdateAirtableRecord<CreditCardTransaction> for CreditCardTransaction {
    async fn update_airtable_record(&mut self, _record: CreditCardTransaction) -> Result<()> {
        Ok(())
    }
}

pub async fn refresh_ramp_transactions(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    // Create the Ramp client.
    let ramp = company.authenticate_ramp()?;

    // List all our users.
    let users = ramp.list_provider_users(company).await?;
    let mut ramp_users: HashMap<String, String> = Default::default();
    for user in users {
        ramp_users.insert(format!("{}{}", user.first_name, user.last_name), user.email.to_string());
    }

    let transactions = ramp
        .transactions()
        .list(&ramp_minimal_api::ListTransactionsQuery::default())
        .await?;
    for transaction in transactions.data {
        let mut attachments = Vec::new();
        // Get the reciept for the transaction, if they exist.
        for receipt_id in transaction.receipts {
            let receipt = ramp.receipts().get(&receipt_id.to_string()).await?;
            attachments.push(receipt.receipt_url.to_string());
        }

        // Get the user's email for the transaction.
        let email = ramp_users
            .get(&format!(
                "{}{}",
                transaction.card_holder.first_name, transaction.card_holder.last_name
            ))
            .unwrap();

        let mut link_to_vendor: Vec<String> = Default::default();
        let vendor = clean_vendor_name(&transaction.merchant_name, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        let nt = NewCreditCardTransaction {
            transaction_id: transaction.id.to_string(),
            card_vendor: "Ramp".to_string(),
            employee_email: email.to_string(),
            amount: transaction.amount as f32,
            category_id: transaction.sk_category_id as i32,
            category_name: transaction.sk_category_name.to_string(),
            merchant_id: transaction.merchant_id.to_string(),
            merchant_name: transaction.merchant_name.to_string(),
            state: transaction.state.to_string(),
            receipts: attachments,
            card_id: transaction.card_id.to_string(),
            time: transaction.user_transaction_time.unwrap(),
            memo: String::new(),
            link_to_vendor,
            cio_company_id: company.id,
        };

        nt.upsert(db).await?;
    }

    CreditCardTransactions::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

pub async fn refresh_ramp_reimbursements(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    // Create the Ramp client.
    let ramp = company.authenticate_ramp()?;

    // List all our users.
    let users = ramp.list_provider_users(company).await?;
    let mut ramp_users: HashMap<String, String> = Default::default();
    for user in users {
        ramp_users.insert(user.id.to_string(), user.email.to_string());
    }

    let reimbursements = ramp.reimbursements().list().await?;
    for reimbursement in reimbursements.data {
        let mut attachments = Vec::new();
        // Get the reciepts for the reimbursement, if they exist.
        for receipt_id in reimbursement.receipts {
            let receipt = ramp.receipts().get(&receipt_id.to_string()).await?;
            attachments.push(receipt.receipt_url.to_string());
        }

        let merchant = reimbursement.merchant.unwrap_or_default();

        // Get the user's email for the reimbursement.
        let email = ramp_users.get(&reimbursement.user_id).unwrap();

        let mut link_to_vendor: Vec<String> = Default::default();
        let vendor = clean_vendor_name(&merchant, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        let nt = NewExpensedItem {
            transaction_id: reimbursement.id.to_string(),
            expenses_vendor: "Ramp".to_string(),
            employee_email: email.to_string(),
            amount: reimbursement.amount as f32,
            category_id: 0,
            category_name: "".to_string(),
            merchant_id: "".to_string(),
            merchant_name: merchant,
            state: "CLEARED".to_string(),
            receipts: attachments,
            card_id: "".to_string(),
            time: reimbursement.created_at.unwrap(),
            memo: String::new(),
            link_to_vendor,
            cio_company_id: company.id,
        };

        nt.upsert(db).await?;
    }

    ExpensedItems::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

// Changes the vendor name to one that matches our existing list.
fn clean_vendor_name(vendor_name: &str, config: &FinanceConfig) -> String {
    if let Some(alias) = config.vendor_aliases.get(vendor_name) {
        alias.to_string()
    } else {
        vendor_name.to_string()
    }
}

/// Read the Brex transactions from a csv.
/// We don't run this except locally.
pub async fn refresh_brex_transactions(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    let mut path = env::current_dir()?;
    path.push("brex.csv");

    if !path.exists() {
        // Return early the path does not exist.
        info!("brex csv at {} does not exist, returning early", path.to_str().unwrap());
        return Ok(());
    }

    info!("reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path)?;
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewCreditCardTransaction = result?;
        record.card_vendor = "Brex".to_string();
        record.state = "CLEARED".to_string();

        // Parse the user's last name.
        // We stored it in the merchant ID as a hack.
        let name = record.merchant_id.trim().to_string();
        let split = name.split(' ');
        let vec: Vec<&str> = split.collect();
        // Get the last item in the vector.
        let last_name = vec.last().unwrap().to_string();

        // Reset the merchand id so it is clean.
        record.merchant_id = "".to_string();

        // Try to get the user by their last name.
        match users::dsl::users
            .filter(
                users::dsl::last_name
                    .eq(last_name.to_string())
                    .and(users::dsl::cio_company_id.eq(company.id)),
            )
            .first_async::<User>(db.pool())
            .await
        {
            Ok(user) => {
                // Set the user's email.
                record.employee_email = user.email;
            }
            Err(e) => {
                if let Some(email) = config.legacy_expensify.aliases.get(&last_name) {
                    record.employee_email = email.to_string();
                } else {
                    warn!(
                        "could not find user with name `{}` last name `{}`: {}",
                        name, last_name, e
                    );
                }
            }
        }

        // Make sure we have a transaction id.
        if record.transaction_id.is_empty() {
            warn!("transaction_id is missing: {:?}", record);
            // We don't want to save it to our database.
            continue;
        }

        // Try to link to the correct vendor.
        let vendor = clean_vendor_name(&record.merchant_name, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                record.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        record.cio_company_id = company.id;

        // Let's add the record to our database.
        record.upsert(db).await?;
    }

    Ok(())
}

#[db {
    new_struct_name = "AccountsPayable",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_ACCOUNTS_PAYABLE_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "confirmation_number" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = accounts_payables)]
pub struct NewAccountsPayable {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "CONFIRMATION #")]
    pub confirmation_number: String,
    #[serde(default)]
    pub amount: f32,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "INVOICE #")]
    pub invoice_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "VENDOR")]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "CURRENCY")]
    pub currency: String,
    #[serde(alias = "PROCESS DATE", deserialize_with = "bill_com_date_format::deserialize")]
    pub date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "PAYMENT TYPE")]
    pub payment_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "PAYMENT STATUS")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "PAYMENT AMOUNT")]
    pub notes: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_array_of_strings::deserialize",
        serialize_with = "airtable_api::attachment_format_as_array_of_strings::serialize"
    )]
    pub invoices: Vec<String>,
    /// This is linked to another table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_vendor: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a AccountsPayable.
#[async_trait]
impl UpdateAirtableRecord<AccountsPayable> for AccountsPayable {
    async fn update_airtable_record(&mut self, _record: AccountsPayable) -> Result<()> {
        Ok(())
    }
}

pub mod bill_com_date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer};

    const FORMAT: &str = "%m/%d/%y";

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();
        Ok(NaiveDate::parse_from_str(&s, FORMAT).unwrap_or_else(|_| crate::utils::default_date()))
    }
}

/// Sync accounts payable.
pub async fn refresh_accounts_payable(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<AccountsPayable>> = company
        .authenticate_airtable(&company.airtable_base_id_finance)
        .list_records(&AccountsPayable::airtable_table(), "Grid view", vec![])
        .await?;
    for bill_record in results {
        let mut bill: NewAccountsPayable = bill_record.fields.into();

        let vendor = clean_vendor_name(&bill.vendor, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                bill.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        // Upsert the record in our database.
        let mut db_bill = bill.upsert_in_db(db).await?;

        db_bill.cio_company_id = company.id;

        if db_bill.airtable_record_id.is_empty() {
            db_bill.airtable_record_id = bill_record.id;
        }

        db_bill.update(db).await?;
    }

    AccountsPayables::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    Ok(())
}

#[db {
    new_struct_name = "ExpensedItem",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_EXPENSED_ITEMS_TABLE",
    match_on = {
        "cio_company_id" = "i32",
        "transaction_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[diesel(table_name = expensed_items)]
pub struct NewExpensedItem {
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Id")]
    pub transaction_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expenses_vendor: String,
    #[serde(default, alias = "Amount")]
    pub amount: f32,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub employee_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Receipt")]
    pub card_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Attendees")]
    pub merchant_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Merchant")]
    pub merchant_name: String,
    #[serde(default)]
    pub category_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Category")]
    pub category_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "Description")]
    pub memo: String,
    #[serde(alias = "Timestamp")]
    pub time: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "airtable_api::attachment_format_as_array_of_strings::deserialize",
        serialize_with = "airtable_api::attachment_format_as_array_of_strings::serialize"
    )]
    pub receipts: Vec<String>,
    /// This is linked to another table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_vendor: Vec<String>,
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a ExpensedItem.
#[async_trait]
impl UpdateAirtableRecord<ExpensedItem> for ExpensedItem {
    async fn update_airtable_record(&mut self, _record: ExpensedItem) -> Result<()> {
        Ok(())
    }
}

/// Read the Expensify transactions from a csv.
/// We don't run this except locally.
pub async fn refresh_expensify_transactions(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    ExpensedItems::get_from_db(db, company.id)
        .await?
        .update_airtable(db)
        .await?;

    let mut path = env::current_dir()?;
    path.push("expensify.csv");

    if !path.exists() {
        // Return early the path does not exist.
        info!(
            "expensify csv at {} does not exist, returning early",
            path.to_str().unwrap()
        );
        return Ok(());
    }

    info!("reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path)?;
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewExpensedItem = result?;
        record.expenses_vendor = "Expensify".to_string();
        record.state = "CLEARED".to_string();
        record.cio_company_id = company.id;

        // Parse the user's last name.
        // We stored it in the merchant ID as a hack.
        let name = record.merchant_id.trim().to_string();
        let split = name.split(' ');
        let vec: Vec<&str> = split.collect();
        // Get the last item in the vector.
        let last_name = vec.last().unwrap().trim_end_matches("@oxidecomputer.com").to_string();

        // Reset the merchand id so it is clean.
        record.merchant_id = "".to_string();

        // Try to get the user by their last name.
        match users::dsl::users
            .filter(
                users::dsl::last_name
                    .eq(last_name.to_string())
                    .or(users::dsl::username.eq(last_name.to_string())),
            )
            .filter(users::dsl::cio_company_id.eq(company.id))
            .first_async::<User>(db.pool())
            .await
        {
            Ok(user) => {
                // Set the user's email.
                record.employee_email = user.email;
            }
            Err(e) => {
                if let Some(email) = config.legacy_expensify.aliases.get(&last_name) {
                    record.employee_email = email.to_string();
                } else {
                    warn!(
                        "could not find user with name `{}` last name `{}`: {}",
                        name, last_name, e
                    );
                }
            }
        }

        // Grab the card_id and set it as part of receipts.
        if !record.card_id.is_empty()
            && !config
                .legacy_expensify
                .emails_to_exclude
                .contains(&record.employee_email)
        {
            // Get the URL.
            let body = reqwest::get(&record.card_id).await?.text().await?;
            let split = body.split(' ');
            let vec: Vec<&str> = split.collect();

            for word in vec {
                if word.contains("https://www.expensify.com/receipts/")
                    || word.contains("https://s3.amazonaws.com/receipts.expensify.com/")
                {
                    let receipt = word
                        .trim_start_matches("href=\"")
                        .trim_end_matches("\">Download")
                        .to_string();
                    record.receipts = vec![receipt.to_string()];

                    // Stop the loop.
                    break;
                }
            }

            // Reset the card id.
            record.card_id = String::new();
        }

        // Make sure we have a transaction id.
        if record.transaction_id.is_empty() {
            warn!("transaction_id is missing: {:?}", record);
            // We don't want to save it to our database.
            continue;
        }

        // Try to link to the correct vendor.
        let vendor = clean_vendor_name(&record.merchant_name, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                record.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        // Let's add the record to our database.
        record.upsert(db).await?;
    }

    Ok(())
}

/// Read the Bill.com payments from a csv.
/// We don't run this except locally.
pub async fn refresh_bill_com_transactions(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    let mut path = env::current_dir()?;
    path.push("bill.com.csv");

    if !path.exists() {
        // Return early the path does not exist.
        info!(
            "bill.com csv at {} does not exist, returning early",
            path.to_str().unwrap()
        );
        return Ok(());
    }

    info!("reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path)?;
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewAccountsPayable = result?;

        // Get the amount from the notes.
        let sa = record.notes.replace(['$', ','], "");
        record.amount = sa.parse::<f32>()?;
        record.notes = "".to_string();

        // Make sure we have a transaction id.
        if record.confirmation_number.is_empty() {
            warn!("transaction_id is missing: {:?}", record);
            // We don't want to save it to our database.
            continue;
        }

        // Try to link to the correct vendor.
        let vendor = clean_vendor_name(&record.vendor, config);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(db, company.id, vendor.to_string()).await {
            Some(v) => {
                record.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                info!("could not find vendor that matches {}", vendor);
            }
        }

        record.cio_company_id = company.id;

        // Let's add the record to our database.
        record.upsert(db).await?;
    }

    Ok(())
}

pub async fn sync_quickbooks(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    // Authenticate QuickBooks.
    let qba = company.authenticate_quickbooks(db).await;
    if let Err(e) = qba {
        if e.to_string().contains("no token") {
            // Return early, this company does not use QuickBooks.
            return Ok(());
        }

        bail!("authenticating quickbooks failed: {}", e);
    }
    let qb = qba?;

    let bill_payments = qb.list_bill_payments().await?;
    for bill_payment in bill_payments {
        // Let's check if there are any attachments.
        let attachments = qb.list_attachments_for_bill_payment(&bill_payment.id).await?;

        if (attachments.is_empty() && bill_payment.line.is_empty()) || bill_payment.total_amt == 0.0 {
            // Continue early if we have no lines on the bill.
            continue;
        }

        let merchant_name = bill_payment.vendor_ref.name.to_string();
        match accounts_payables::dsl::accounts_payables
            .filter(
                accounts_payables::dsl::vendor
                    .eq(merchant_name.to_string())
                    .and(accounts_payables::dsl::amount.eq(bill_payment.total_amt))
                    .and(accounts_payables::dsl::date.eq(bill_payment.txn_date)),
            )
            .first_async::<AccountsPayable>(db.pool())
            .await
        {
            Ok(mut transaction) => {
                // Add the receipt.
                // Clear out existing invoices.
                transaction.invoices = vec![];
                for line in bill_payment.line {
                    // Iterate over each of the linked transactions.
                    for txn in line.linked_txn {
                        if txn.txn_type == "Bill" {
                            // Get the bill.
                            let bill = qb.get_bill(&txn.txn_id).await?;
                            // Get the attachments for the bill.
                            let attachments = qb.list_attachments_for_bill(&bill.id).await?;
                            for attachment in attachments {
                                transaction.invoices.push(attachment.temp_download_uri.to_string());
                            }
                        }
                    }
                }
                transaction.cio_company_id = company.id;

                transaction.update(db).await?;
                continue;
            }
            Err(e) => {
                info!(
                    "could not find transaction with merchant_name `{}` -> `{}` amount `{}`  date `{}`: {}",
                    bill_payment.vendor_ref.name, merchant_name, bill_payment.total_amt, bill_payment.txn_date, e
                );
            }
        }
    }

    let purchases = qb.list_purchases().await?;
    for purchase in purchases.clone() {
        // Let's try to match the Brex reciepts to the transactions.
        if purchase.account_ref.name == "Credit Cards:Brex" {
            // See if we even have attachments.
            let attachments = qb.list_attachments_for_purchase(&purchase.id).await?;
            if attachments.is_empty() {
                // We can continue early since we don't have attachments.
                continue;
            }

            // This is a brex transaction, let's try to find it in our database to update it.
            // We know we have attachments as well.
            let time_start = NaiveTime::from_hms_milli(0, 0, 0, 0);
            let sdt = purchase
                .txn_date
                .checked_sub_signed(Duration::days(10))
                .unwrap()
                .and_time(time_start);
            let time_end = NaiveTime::from_hms_milli(23, 59, 59, 59);
            let edt = purchase.txn_date.and_time(time_end);
            let merchant_name = clean_merchant_name(&purchase.entity_ref.name, config);
            match credit_card_transactions::dsl::credit_card_transactions
                .filter(
                    credit_card_transactions::dsl::merchant_name
                        .eq(merchant_name.to_string())
                        .and(credit_card_transactions::dsl::card_vendor.eq("Brex".to_string()))
                        .and(credit_card_transactions::dsl::amount.eq(purchase.total_amt))
                        .and(credit_card_transactions::dsl::time.ge(DateTime::<Utc>::from_utc(sdt, Utc)))
                        .and(credit_card_transactions::dsl::time.le(DateTime::<Utc>::from_utc(edt, Utc))),
                )
                .first_async::<CreditCardTransaction>(db.pool())
                .await
            {
                Ok(mut transaction) => {
                    // Add the receipt.
                    // Clear out existing receipts.
                    transaction.receipts = vec![];
                    for attachment in attachments {
                        transaction.receipts.push(attachment.temp_download_uri.to_string());
                    }
                    transaction.update(db).await?;
                    continue;
                }
                Err(e) => {
                    info!(
                        "could not find transaction with merchant_name `{}` -> `{}` amount `{}` date `{}` --> less than `{}` greater than `{}`: {}",
                        purchase.entity_ref.name, merchant_name, purchase.total_amt, purchase.txn_date, sdt, edt, e
                    );
                }
            }
        }
    }

    Ok(())
}

fn clean_merchant_name(merchant_name: &str, config: &FinanceConfig) -> String {
    if let Some(alias) = config.merchant_aliases.get(merchant_name) {
        alias.to_string()
    } else {
        merchant_name.to_string()
    }
}

pub async fn refresh_all_finance(db: &Database, company: &Company, config: &FinanceConfig) -> Result<()> {
    let (sv, reim, trans, ap, qb) = tokio::join!(
        refresh_software_vendors(db, company),
        refresh_ramp_reimbursements(db, company, config),
        refresh_ramp_transactions(db, company, config),
        refresh_accounts_payable(db, company, config),
        sync_quickbooks(db, company, config),
    );

    sv?;
    reim?;
    trans?;
    ap?;
    qb?;

    Ok(())
}
