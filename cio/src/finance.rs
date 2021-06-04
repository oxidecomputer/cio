use std::collections::HashMap;
use std::env;
use std::fs::File;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use gsuite_api::GSuite;
use macros::db;
use okta::Okta;
use ramp_api::Ramp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_ACCOUNTS_PAYABLE_TABLE, AIRTABLE_BASE_ID_FINANCE, AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE, AIRTABLE_SOFTWARE_VENDORS_TABLE};
use crate::configs::{Group, User};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{accounts_payables, credit_card_transactions, software_vendors, users};
use crate::utils::{authenticate_github_jwt, get_gsuite_token, github_org, GSUITE_DOMAIN};

#[db {
    new_struct_name = "SoftwareVendor",
    airtable_base_id = "AIRTABLE_BASE_ID_FINANCE",
    airtable_table = "AIRTABLE_SOFTWARE_VENDORS_TABLE",
    match_on = {
        "name" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "software_vendors"]
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
}

/// This is only used for serialize
fn is_zero(num: &f32) -> bool {
    *num == 0.0
}

/// Implement updating the Airtable record for a SoftwareVendor.
#[async_trait]
impl UpdateAirtableRecord<SoftwareVendor> for SoftwareVendor {
    async fn update_airtable_record(&mut self, record: SoftwareVendor) {
        // This is a function so we can't change it through the API.
        self.total_cost_per_month = 0.0;
        // Keep this the same, we update it from the transactions.
        self.link_to_transactions = record.link_to_transactions;
        // Keep this the same, we update it from the accounts payable.
        self.link_to_accounts_payable = record.link_to_accounts_payable;
    }
}

/// Sync software vendors from Airtable.
pub async fn refresh_software_vendors() {
    let gsuite_customer = env::var("GADMIN_ACCOUNT_ID").unwrap();
    let token = get_gsuite_token("").await;
    let gsuite = GSuite::new(&gsuite_customer, GSUITE_DOMAIN, token.clone());

    let db = Database::new();

    let github = authenticate_github_jwt();

    let okta = Okta::new_from_env();

    let slack = slack_chat_api::Slack::new_from_env();

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SoftwareVendor>> = SoftwareVendor::airtable().list_records(&SoftwareVendor::airtable_table(), "Grid view", vec![]).await.unwrap();
    for vendor_record in results {
        let mut vendor: NewSoftwareVendor = vendor_record.fields.into();

        if vendor.name == "GitHub" {
            // Update the number of GitHub users in our org.
            let org = github.org(github_org()).get().await.unwrap();
            vendor.users = org.plan.filled_seats;
        }

        if vendor.name == "Okta" {
            let users = okta.list_users().await.unwrap();
            vendor.users = users.len() as i32;
        }

        if vendor.name == "Google Workspace" {
            let users = gsuite.list_users().await.unwrap();
            vendor.users = users.len() as i32;
        }

        if vendor.name == "Slack" {
            let users = slack.billable_info().await.unwrap();
            let mut count = 0;
            for (_, user) in users {
                if user.billing_active {
                    count += 1;
                }
            }

            vendor.users = count;
        }

        // Airtable, Brex, Gusto, Expensify are all the same number of users as
        // in all@.
        if vendor.name == "Airtable" || vendor.name == "Ramp" || vendor.name == "Brex" || vendor.name == "Gusto" || vendor.name == "Expensify" {
            let group = Group::get_from_db(&db, "all".to_string()).unwrap();
            let airtable_group = group.get_existing_airtable_record().await.unwrap();
            vendor.users = airtable_group.fields.members.len() as i32;
        }

        // Upsert the record in our database.
        let mut db_vendor = vendor.upsert_in_db(&db);

        if db_vendor.airtable_record_id.is_empty() {
            db_vendor.airtable_record_id = vendor_record.id;
        }

        // Update the cost per month.
        db_vendor.total_cost_per_month = (db_vendor.cost_per_user_per_month * db_vendor.users as f32) + db_vendor.flat_cost_per_month;

        db_vendor.update(&db).await;
    }

    SoftwareVendors::get_from_db(&db).update_airtable().await
}

#[db {
    new_struct_name = "CreditCardTransaction",
    airtable_base_id = "AIRTABLE_BASE_ID_FINANCE",
    airtable_table = "AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE",
    match_on = {
        "transaction_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "credit_card_transactions"]
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
}

/// Implement updating the Airtable record for a CreditCardTransaction.
#[async_trait]
impl UpdateAirtableRecord<CreditCardTransaction> for CreditCardTransaction {
    async fn update_airtable_record(&mut self, _record: CreditCardTransaction) {}
}

pub async fn refresh_ramp_transactions() {
    // Create the Ramp client.
    let ramp = Ramp::new_from_env().await;

    // Initialize the database.
    let db = Database::new();

    // List all our users.
    let users = ramp.list_users().await.unwrap();
    let mut ramp_users: HashMap<String, String> = Default::default();
    for user in users {
        ramp_users.insert(format!("{}{}", user.first_name, user.last_name), user.email.to_string());
    }

    let transactions = ramp.get_transactions().await.unwrap();
    for transaction in transactions {
        let mut attachments = Vec::new();
        // Get the reciept for the transaction, if they exist.
        for receipt_id in transaction.receipts {
            let receipt = ramp.get_receipt(&receipt_id).await.unwrap();
            attachments.push(receipt.receipt_url.to_string());
        }

        // Get the user's email for the transaction.
        let email = ramp_users.get(&format!("{}{}", transaction.card_holder.first_name, transaction.card_holder.last_name)).unwrap();

        let mut link_to_vendor: Vec<String> = Default::default();
        let vendor = clean_vendor_name(&transaction.merchant_name);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(&db, vendor.to_string()) {
            Some(v) => {
                link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                println!("could not find vendor that matches {}", vendor);
            }
        }

        let nt = NewCreditCardTransaction {
            transaction_id: transaction.id,
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
            time: transaction.user_transaction_time,
            memo: String::new(),
            link_to_vendor,
        };

        nt.upsert(&db).await;
    }

    CreditCardTransactions::get_from_db(&db).update_airtable().await;
}

// Changes the vendor name to one that matches our existing list.
fn clean_vendor_name(s: &str) -> String {
    if s == "Clara Labs" {
        "Claralabs".to_string()
    } else if (s.contains("Paypal") && (s.ends_with("Eb") || s.contains("Ebay") || s.ends_with("Eba")))
        || s == "Ebay"
        || s == "Paypal Transaction Allknagoods"
        || s == "Paypal Transaction Intuitimage"
        || s == "Paypal Transaction Djjrubs"
        || s == "PayPal Transaction - Frantiques"
    {
        "eBay".to_string()
    } else if s == "Creative Safety Supply LLC" {
        "Creative Safety Supply".to_string()
    } else if s == "USENIX Association" {
        "USENIX".to_string()
    } else if s == "Grubhub" {
        "GrubHub".to_string()
    } else if s == "Amazon Web Services" {
        "AWS".to_string()
    } else if s == "Ubiquiti Labs, Llc" || s == "Ubiquiti Inc." || s == "Ubiquiti Networks" {
        "Ubiquiti".to_string()
    } else if s == "IEEE Standards Association" || s == "IEEE SA - Products & Services" {
        "IEEE".to_string()
    } else if s == "Solarwinds" {
        "Pingdom".to_string()
    } else if s == "GoTanscript" || s == "PAYPAL *GOTRANSCRIP" {
        "GoTranscript".to_string()
    } else if s == "Chelsio Communications" {
        "Chelsio".to_string()
    } else if s == "The Linux Foundation" {
        "Linux Foundation".to_string()
    } else if s == "SparkFun Electronics" {
        "SparkFun".to_string()
    } else if s == "Google G Suite" {
        "Google Workspace".to_string()
    } else if s == "Atlassian" || s == "Atlassian Statuspage" {
        "Statuspage.io".to_string()
    } else if s == "Digitalocean" {
        "DigitalOcean".to_string()
    } else if s == "Rev.com" {
        "Rev.ai".to_string()
    } else if s == "Intuit Quickbooks" || s == "Intuit" {
        "QuickBooks".to_string()
    } else if s == "Github" {
        "GitHub".to_string()
    } else if s == "Texas Instruments Incorpo" {
        "Texas Instruments".to_string()
    } else if s == "Packlane, Inc." {
        "Packlane".to_string()
    } else if s == "Yeti" {
        "YETI".to_string()
    } else if s == "TaskRabbit Support" || s == "Paypal Transaction - Fadi_jaber88" || s == "Venmo" {
        "TaskRabbit".to_string()
    } else if s == "Dell Inc" {
        "Dell".to_string()
    } else if s == "Sonix AI" {
        "Sonix.ai".to_string()
    } else if s == "WPG Americas Inc" {
        "WPG Americas".to_string()
    } else if s == "PAYPAL *PC ENGINES" {
        "PC Engines".to_string()
    } else if s == "The Linley Group" {
        "Linley Group".to_string()
    } else if s == "Finisar Corporation" {
        "Finisar".to_string()
    } else if s == "AISense, Inc." {
        "Otter.ai".to_string()
    } else if s == "Amazon Business Prime" || s == "Amzn Mktp Uk" || s == "Amazon Digital Services" || s == "Amazon.com" {
        "Amazon".to_string()
    } else if s == "FS.COM - Fiberstore" {
        "Fiber Store".to_string()
    } else if s == "FAX.PLUS" || s == "FAXPLUS" {
        "Fax.plus".to_string()
    } else if s == "The Container Store" {
        "Container Store".to_string()
    } else if s == "Avnet Electronics" {
        "Avnet".to_string()
    } else if s == "lululemon" {
        "Lululemon".to_string()
    } else if s == "HP Store" || s == "HP" {
        "Hewlett Packard".to_string()
    } else if s == "The UPS Store" {
        "UPS".to_string()
    } else if s == "Microchip Technology" {
        "Microchip".to_string()
    } else if s == "Mouser Electronics" {
        "Mouser".to_string()
    } else if s == "Amphenol Cables on Demand" {
        "Amphenol".to_string()
    } else if s == "Pcbway" || s == "pcbway" {
        "PCBWay".to_string()
    } else if s == "Pccablescom Inc" {
        "PC Cables".to_string()
    } else if s == "UL Standards Sales Site" {
        "UL Standards".to_string()
    } else if s == "Elektronik Billiger Ug" {
        "Elektronik Billiger".to_string()
    } else if s == "Saleae, Inc." {
        "Saleae".to_string()
    } else if s == "DigiKey Electronics" {
        "Digi-Key".to_string()
    } else if s == "GANDI.net" {
        "Gandi.net".to_string()
    } else if s == "Temi.com" {
        "Temi".to_string()
    } else if s == "Tequipment" || s == "Tequipment.net" {
        "TEquipment".to_string()
    } else if s == "1-800-GOT-JUNK?" {
        "Junk Removal".to_string()
    } else if s == "Intuit Transaction - Fiberopticcablesho" {
        "Fiber Optic Cable Shop".to_string()
    } else if s == "ZEIT" {
        "Vercel".to_string()
    } else if s == "FTDI Chipshop USA" {
        "FTDI Chip".to_string()
    } else if s == "RS COMPONENTS LTD" {
        "RS Components".to_string()
    } else if s == "Pearson Education" {
        "Pearson".to_string()
    } else if s == "Paypal - Sensepeekab" {
        "Sensepeek".to_string()
    } else if s == "TAILSCALE" {
        "Tailscale".to_string()
    } else if s == "Formidable Labs, LLC" {
        "Formidable".to_string()
    } else if s == "YouTube Premium" {
        "YouTube".to_string()
    } else if s == "Mindshare Benefits & Insurance Service, Inc" {
        "Mindshare".to_string()
    } else if s == "Future Electronics Corp (MA)" {
        "Future Electronics".to_string()
    } else if s == "Zoom.us" || s == "Zoom Video Communications" {
        "Zoom".to_string()
    } else if s == "Hardware Security Training and Research" {
        "Hardware Security Training".to_string()
    } else if s == "Rudys Cant Fail Cafe" {
        "Rudy's Can't Fail Cafe".to_string()
    } else if s == "The Home Depot" {
        "Home Depot".to_string()
    } else if s == "Owl Lads" {
        "Owl Labs".to_string()
    } else if s == "PITCH.COM" {
        "Pitch".to_string()
    } else if s == "Intel Corporation" {
        "Intel".to_string()
    } else if s == "Advanced Micro Devices, Inc." {
        "AMD".to_string()
    } else if s == "Benchmark Electronics, Inc." {
        "Benchmark".to_string()
    } else if s == "HumblePod LLC" || s == "HumblePod" {
        "Chris Hill".to_string()
    } else if s == "Kruze Consulting, Inc." {
        "Kruze".to_string()
    } else if s == "Okta Inc" {
        "Okta".to_string()
    } else if s == "EMA Design Automation" {
        "Cadence".to_string()
    } else if s == "FOLGER LEVIN LLP" {
        "Folger Levin".to_string()
    } else if s == "Sager Electronics" {
        "Sager".to_string()
    } else if s == "Morrison & Foerster LLP" {
        "Morrison & Foerster".to_string()
    } else if s == "Spec" {
        "John McMaster".to_string()
    } else if s == "JN Engineering LLC" {
        "Jon Nydell".to_string()
    } else if s == "Wiwynn International Corp" {
        "Wiwynn".to_string()
    } else if s == "Tyan Computer Corporation" {
        "TYAN".to_string()
    } else if s == "510 Investments LLC" {
        "510 Investments".to_string()
    } else if s == "The Association for Computing Machinery" || s == "Association for Computing Machinery" {
        "ACM".to_string()
    } else if s == "LATHAM&WATKINS" {
        "Latham & Watkins".to_string()
    } else if s == "cleverbridge" || s == "Cleverbridge" {
        "Parallels".to_string()
    } else {
        s.to_string()
    }
}

/// Read the Brex transactions from a csv.
/// We don't run this except locally.
pub async fn refresh_brex_transactions() {
    // Initialize the database.
    let db = Database::new();

    let mut path = env::current_dir().unwrap();
    path.push("brex.csv");

    if !path.exists() {
        // Return early the path does not exist.
        println!("Brex csv at {} does not exist, returning early", path.to_str().unwrap());
        return;
    }

    println!("Reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path).unwrap();
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewCreditCardTransaction = result.unwrap();
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
        match users::dsl::users.filter(users::dsl::last_name.eq(last_name.to_string())).first::<User>(&db.conn()) {
            Ok(user) => {
                // Set the user's email.
                record.employee_email = user.email();
            }
            Err(e) => {
                if last_name == "Volpe" {
                    record.employee_email = "jared@oxidecomputer.com".to_string();
                    continue;
                } else if last_name == "Randal" {
                    record.employee_email = "allison@oxidecomputer.com".to_string();
                    continue;
                }

                println!("could not find user with name `{}` last name `{}`: {}", name, last_name, e);
            }
        }

        // Make sure we have a transaction id.
        if record.transaction_id.is_empty() {
            println!("transaction_id is missing: {:?}", record);
            // We don't want to save it to our database.
            continue;
        }

        // Try to link to the correct vendor.
        let vendor = clean_vendor_name(&record.merchant_name);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(&db, vendor.to_string()) {
            Some(v) => {
                record.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                println!("could not find vendor that matches {}", vendor);
            }
        }

        // Let's add the record to our database.
        record.upsert(&db).await;
    }
}

#[db {
    new_struct_name = "AccountsPayable",
    airtable_base_id = "AIRTABLE_BASE_ID_FINANCE",
    airtable_table = "AIRTABLE_ACCOUNTS_PAYABLE_TABLE",
    match_on = {
        "confirmation_number" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "accounts_payables"]
pub struct NewAccountsPayable {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confirmation_number: String,
    #[serde(default)]
    pub amount: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub invoice_number: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
    pub date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payment_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// This is linked to another table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_to_vendor: Vec<String>,
}

/// Implement updating the Airtable record for a AccountsPayable.
#[async_trait]
impl UpdateAirtableRecord<AccountsPayable> for AccountsPayable {
    async fn update_airtable_record(&mut self, _record: AccountsPayable) {}
}

/// Sync accounts payable.
pub async fn refresh_accounts_payable() {
    let db = Database::new();

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<AccountsPayable>> = AccountsPayable::airtable().list_records(&AccountsPayable::airtable_table(), "Grid view", vec![]).await.unwrap();
    for bill_record in results {
        let mut bill: NewAccountsPayable = bill_record.fields.into();

        let vendor = clean_vendor_name(&bill.vendor);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(&db, vendor.to_string()) {
            Some(v) => {
                bill.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                println!("could not find vendor that matches {}", vendor);
            }
        }

        // Upsert the record in our database.
        let mut db_bill = bill.upsert_in_db(&db);

        if db_bill.airtable_record_id.is_empty() {
            db_bill.airtable_record_id = bill_record.id;
        }

        db_bill.update(&db).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::finance::{refresh_accounts_payable, refresh_brex_transactions, refresh_ramp_transactions, refresh_software_vendors};
    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_brex() {
        refresh_brex_transactions().await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_finance() {
        refresh_software_vendors().await;

        refresh_accounts_payable().await;

        refresh_ramp_transactions().await;
    }
}
