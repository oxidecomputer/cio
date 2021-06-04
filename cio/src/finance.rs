use std::collections::HashMap;
use std::env;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use gsuite_api::GSuite;
use macros::db;
use okta::Okta;
use ramp_api::Ramp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_ACCOUNTS_PAYABLE_TABLE, AIRTABLE_BASE_ID_FINANCE, AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE, AIRTABLE_SOFTWARE_VENDORS_TABLE};
use crate::configs::Group;
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{accounts_payables, credit_card_transactions, software_vendors};
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
}

#[db {
    new_struct_name = "CreditCardTransaction",
    airtable_base_id = "AIRTABLE_BASE_ID_FINANCE",
    airtable_table = "AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE",
    match_on = {
        "ramp_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "credit_card_transactions"]
pub struct NewCreditCardTransaction {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ramp_id: String,
    #[serde(default)]
    pub amount: f32,
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        serialize_with = "airtable_api::user_format_as_string::serialize",
        deserialize_with = "airtable_api::user_format_as_string::deserialize"
    )]
    pub employee_email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub card_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merchant_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merchant_name: String,
    #[serde(default)]
    pub category_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
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

pub async fn refresh_transactions() {
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
            ramp_id: transaction.id,
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
            link_to_vendor,
        };

        nt.upsert(&db).await;
    }
}

// Changes the vendor name to one that matches our existing list.
fn clean_vendor_name(s: &str) -> String {
    if s == "Clara Labs" {
        "Claralabs".to_string()
    } else if s == "Ubiquiti Labs, Llc" {
        "Ubiquiti".to_string()
    } else if s == "Google G Suite" {
        "Google Workspace".to_string()
    } else if s == "Atlassian" {
        "Statuspage.io".to_string()
    } else if s == "Rev.com" {
        "Rev.ai".to_string()
    } else if s == "Intuit Quickbooks" {
        "QuickBooks".to_string()
    } else if s == "Github" {
        "GitHub".to_string()
    } else if s == "Texas Instruments Incorpo" {
        "Texas Instruments".to_string()
    } else if s == "Packlane, Inc." {
        "Packlane".to_string()
    } else if s == "Yeti" {
        "YETI".to_string()
    } else if s == "TaskRabbit Support" {
        "TaskRabbit".to_string()
    } else if s == "Amazon Business Prime" {
        "Amazon".to_string()
    } else if s == "lululemon" {
        "Lululemon".to_string()
    } else if s == "HP Store" {
        "Hewlett Packard".to_string()
    } else if s == "The UPS Store" {
        "UPS".to_string()
    } else if s == "Microchip Technology" {
        "Microchip".to_string()
    } else if s == "Mouser Electronics" {
        "Mouser".to_string()
    } else if s == "Amphenol Cables on Demand" {
        "Amphenol".to_string()
    } else if s == "Pcbway" {
        "PCBWay".to_string()
    } else if s == "Ebay" {
        "eBay".to_string()
    } else if s == "Pccablescom Inc" {
        "PC Cables".to_string()
    } else if s == "UL Standards Sales Site" {
        "UL Standards".to_string()
    } else if s == "Elektronik Billiger Ug" {
        "Elektronik Billiger".to_string()
    } else if s == "Formidable Labs, LLC" {
        "Formidable".to_string()
    } else if s == "Mindshare Benefits & Insurance Service, Inc" {
        "Mindshare".to_string()
    } else if s == "Future Electronics Corp (MA)" {
        "Future Electronics".to_string()
    } else if s == "Intel Corporation" {
        "Intel".to_string()
    } else if s == "Advanced Micro Devices, Inc." {
        "AMD".to_string()
    } else if s == "Benchmark Electronics, Inc." {
        "Benchmark".to_string()
    } else if s == "HumblePod LLC" {
        "Chris Hill".to_string()
    } else if s == "Kruze Consulting, Inc." {
        "Kruze".to_string()
    } else if s == "Okta Inc" {
        "Okta".to_string()
    } else if s == "EMA Design Automation" {
        "Cadence".to_string()
    } else if s == "FOLGER LEVIN LLP" {
        "Folger Levin".to_string()
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
    } else if s == "LATHAM&WATKINS" {
        "Latham & Watkins".to_string()
    } else {
        s.to_string()
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
    use crate::finance::{refresh_accounts_payable, refresh_software_vendors, refresh_transactions};

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cron_finance() {
        refresh_software_vendors().await;

        refresh_accounts_payable().await;

        refresh_transactions().await;
    }
}
