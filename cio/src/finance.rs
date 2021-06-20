use std::collections::HashMap;
use std::env;
use std::fs::File;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use gsuite_api::GSuite;
use macros::db;
use okta::Okta;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::airtable::{AIRTABLE_ACCOUNTS_PAYABLE_TABLE, AIRTABLE_CREDIT_CARD_TRANSACTIONS_TABLE, AIRTABLE_EXPENSED_ITEMS_TABLE, AIRTABLE_SOFTWARE_VENDORS_TABLE};
use crate::companies::Company;
use crate::configs::{Group, User};
use crate::core::UpdateAirtableRecord;
use crate::db::Database;
use crate::schema::{accounts_payables, credit_card_transactions, expensed_items, software_vendors, users};

#[db {
    new_struct_name = "SoftwareVendor",
    airtable_base = "finance",
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
    async fn update_airtable_record(&mut self, record: SoftwareVendor) {
        // This is a function so we can't change it through the API.
        self.total_cost_per_month = 0.0;
        // Keep this the same, we update it from the transactions.
        self.link_to_transactions = record.link_to_transactions;
        // Keep this the same, we update it from the accounts payable.
        self.link_to_accounts_payable = record.link_to_accounts_payable;
        // Keep this the same, we update it from the expensed items.
        self.link_to_expensed_items = record.link_to_expensed_items;
    }
}

/// Sync software vendors from Airtable.
pub async fn refresh_software_vendors(db: &Database, company: &Company) {
    let token = company.authenticate_google(&db).await;
    let gsuite = GSuite::new(&company.gsuite_account_id, &company.gsuite_domain, token.clone());

    let github = company.authenticate_github();

    let okta = Okta::new(env::var("OKTA_API_TOKEN").unwrap(), &company.okta_domain);

    let slack = slack_chat_api::Slack::new_from_env();

    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<SoftwareVendor>> = company
        .authenticate_airtable(&company.airtable_base_id_finance)
        .list_records(&SoftwareVendor::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
    for vendor_record in results {
        let mut vendor: NewSoftwareVendor = vendor_record.fields.into();

        // Set the company id.
        vendor.cio_company_id = company.id;

        if vendor.name == "GitHub" {
            // Update the number of GitHub users in our org.
            let org = github.org(&company.github_org).get().await.unwrap();
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
            let group = Group::get_from_db(&db, company.id, "all".to_string()).unwrap();
            let airtable_group = group.get_existing_airtable_record(&db).await.unwrap();
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

    SoftwareVendors::get_from_db(&db, company.id).update_airtable(&db).await
}

#[db {
    new_struct_name = "CreditCardTransaction",
    airtable_base = "finance",
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
    /// The CIO company ID.
    #[serde(default)]
    pub cio_company_id: i32,
}

/// Implement updating the Airtable record for a CreditCardTransaction.
#[async_trait]
impl UpdateAirtableRecord<CreditCardTransaction> for CreditCardTransaction {
    async fn update_airtable_record(&mut self, _record: CreditCardTransaction) {}
}

pub async fn refresh_ramp_transactions(db: &Database, company: &Company) {
    // Create the Ramp client.
    let ramp = company.authenticate_ramp(&db).await;

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
            cio_company_id: company.id,
        };

        nt.upsert(&db).await;
    }

    CreditCardTransactions::get_from_db(&db, company.id).update_airtable(&db).await;
}

// Changes the vendor name to one that matches our existing list.
fn clean_vendor_name(s: &str) -> String {
    if s == "Clara Labs" {
        "Claralabs".to_string()
    } else if s == "StickyLife" {
        "Sticky Life".to_string()
    } else if ((s.contains("Paypal") || s.contains("PayPal")) && (s.ends_with("Eb") || s.contains("Ebay") || s.ends_with("Eba")))
        || s == "Ebay"
        || s == "Paypal Transaction Allknagoods"
        || s == "Paypal Transaction Intuitimage"
        || s == "Paypal Transaction Djjrubs"
        || s == "PayPal Transaction - Frantiques"
    {
        "eBay".to_string()
    } else if s == "Gumroad" {
        "Chart".to_string()
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
    } else if s == "Google G Suite" || s == "Google" {
        "Google Workspace".to_string()
    } else if s == "Atlassian" || s == "Atlassian Statuspage" {
        "Statuspage.io".to_string()
    } else if s == "Digitalocean" {
        "DigitalOcean".to_string()
    } else if s == "Rev.com" {
        "Rev.ai".to_string()
    } else if s == "TTI, Inc." {
        "TTI".to_string()
    } else if s == "Intuit Quickbooks" || s == "Intuit" {
        "QuickBooks".to_string()
    } else if s == "Electronics Online" {
        "Maplin".to_string()
    } else if s == "Github" {
        "GitHub".to_string()
    } else if s == "Texas Instruments Incorpo" {
        "Texas Instruments".to_string()
    } else if s == "Packlane, Inc." {
        "Packlane".to_string()
    } else if s == "Yeti" {
        "YETI".to_string()
    } else if s == "keychron.com" {
        "Keychron".to_string()
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
    } else if s == "Dribbble Holdings Ltd." {
        "Dribbble".to_string()
    } else if s == "Saleae, Inc." || s == "SALEAE" {
        "Saleae".to_string()
    } else if s == "DigiKey Electronics" {
        "Digi-Key".to_string()
    } else if s == "McAfee Software" {
        "McAfee".to_string()
    } else if s == "GANDI.net" {
        "Gandi.net".to_string()
    } else if s == "Buywee.com" {
        "BuyWee".to_string()
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
    } else if s == "Eventbrite - Ba-1111 Online Int" {
        "Barefoot Networks Tofino Class".to_string()
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
    } else if s == "Expensify, Inc." {
        "Expensify".to_string()
    } else if s == "Apple Inc." {
        "Apple".to_string()
    } else if s == "Little Snitch Mac Tool" {
        "Little Snitch".to_string()
    } else if s == "VMware" {
        "VMWare".to_string()
    } else if s == "Bakesale Betty's" {
        "Bakesale Betty".to_string()
    } else if s == "Bed Bath and Beyond #26" {
        "Bed Bath and Beyond".to_string()
    } else if s == "Delta Air Lines" || s == "Delta" {
        "Delta Airlines".to_string()
    } else if s == "National Passenger Rail Corporation" || s == "National Passenger Railroad Corporation" {
        "Amtrak".to_string()
    } else if s == "TRINET" || s == "Trinet Cobra" {
        "TriNet".to_string()
    } else if s == "Four Points By Sheraton" || s == "Sheraton Hotels and Resorts" {
        "Four Points by Sheraton San Francisco Bay Bridge".to_string()
    } else if s == "Clipper card" {
        "Clipper".to_string()
    } else if s == "LinkedIn Corporation" {
        "LinkedIn".to_string()
    } else if s == "American Portwell Technology, Inc" {
        "Portwell".to_string()
    } else if s == "PAYPAL *QUICKLUTION QU" {
        "Mail Merge for Avery Labels".to_string()
    } else if s == "Pentagram Design LTD" {
        "Pentagram".to_string()
    } else {
        s.to_string()
    }
}

/// Read the Brex transactions from a csv.
/// We don't run this except locally.
pub async fn refresh_brex_transactions(db: &Database, company: &Company) {
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
        match users::dsl::users
            .filter(users::dsl::last_name.eq(last_name.to_string()).and(users::dsl::cio_company_id.eq(company.id)))
            .first::<User>(&db.conn())
        {
            Ok(user) => {
                // Set the user's email.
                record.employee_email = user.email(company);
            }
            Err(e) => {
                if last_name == "Volpe" {
                    record.employee_email = "jared@oxidecomputer.com".to_string();
                } else if last_name == "Randal" {
                    record.employee_email = "allison@oxidecomputer.com".to_string();
                } else {
                    println!("could not find user with name `{}` last name `{}`: {}", name, last_name, e);
                }
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

        record.cio_company_id = company.id;

        // Let's add the record to our database.
        record.upsert(&db).await;
    }
}

#[db {
    new_struct_name = "AccountsPayable",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_ACCOUNTS_PAYABLE_TABLE",
    match_on = {
        "confirmation_number" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "accounts_payables"]
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
    async fn update_airtable_record(&mut self, _record: AccountsPayable) {}
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
pub async fn refresh_accounts_payable(db: &Database, company: &Company) {
    // Get all the records from Airtable.
    let results: Vec<airtable_api::Record<AccountsPayable>> = company
        .authenticate_airtable(&company.airtable_base_id_finance)
        .list_records(&AccountsPayable::airtable_table(), "Grid view", vec![])
        .await
        .unwrap();
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

        db_bill.cio_company_id = company.id;

        if db_bill.airtable_record_id.is_empty() {
            db_bill.airtable_record_id = bill_record.id;
        }

        db_bill.update(&db).await;
    }

    AccountsPayables::get_from_db(&db, company.id).update_airtable(&db).await;
}

#[db {
    new_struct_name = "ExpensedItem",
    airtable_base = "finance",
    airtable_table = "AIRTABLE_EXPENSED_ITEMS_TABLE",
    match_on = {
        "transaction_id" = "String",
    },
}]
#[derive(Debug, Insertable, AsChangeset, PartialEq, Clone, JsonSchema, Deserialize, Serialize)]
#[table_name = "expensed_items"]
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
    async fn update_airtable_record(&mut self, _record: ExpensedItem) {}
}

/// Read the Expensify transactions from a csv.
/// We don't run this except locally.
pub async fn refresh_expensify_transactions(db: &Database, company: &Company) {
    ExpensedItems::get_from_db(&db, company.id).update_airtable(&db).await;

    let mut path = env::current_dir().unwrap();
    path.push("expensify.csv");

    if !path.exists() {
        // Return early the path does not exist.
        println!("Expensify csv at {} does not exist, returning early", path.to_str().unwrap());
        return;
    }

    println!("Reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path).unwrap();
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewExpensedItem = result.unwrap();
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
            .filter(users::dsl::last_name.eq(last_name.to_string()).or(users::dsl::username.eq(last_name.to_string())))
            .filter(users::dsl::cio_company_id.eq(company.id))
            .first::<User>(&db.conn())
        {
            Ok(user) => {
                // Set the user's email.
                record.employee_email = user.email(&company);
            }
            Err(e) => {
                if last_name == "Volpe" || last_name == "jared" {
                    record.employee_email = "jared@oxidecomputer.com".to_string();
                } else if last_name == "Randal" || last_name == "allison" {
                    record.employee_email = "allison@oxidecomputer.com".to_string();
                } else {
                    println!("could not find user with name `{}` last name `{}`: {}", name, last_name, e);
                }
            }
        }

        // Grab the card_id and set it as part of receipts.
        if !record.card_id.is_empty() && record.employee_email != "allison@oxidecomputer.com" {
            // Get the URL.
            let body = reqwest::get(&record.card_id).await.unwrap().text().await.unwrap();
            let split = body.split(' ');
            let vec: Vec<&str> = split.collect();

            for word in vec {
                if word.contains("https://www.expensify.com/receipts/") || word.contains("https://s3.amazonaws.com/receipts.expensify.com/") {
                    let receipt = word.trim_start_matches("href=\"").trim_end_matches("\">Download").to_string();
                    println!("{}", receipt);
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

/// Read the Bill.com payments from a csv.
/// We don't run this except locally.
pub async fn refresh_bill_com_transactions(db: &Database, company: &Company) {
    let mut path = env::current_dir().unwrap();
    path.push("bill.com.csv");

    if !path.exists() {
        // Return early the path does not exist.
        println!("Bill.com csv at {} does not exist, returning early", path.to_str().unwrap());
        return;
    }

    println!("Reading csv from {}", path.to_str().unwrap());
    let f = File::open(&path).unwrap();
    let mut rdr = csv::Reader::from_reader(f);
    for result in rdr.deserialize() {
        let mut record: NewAccountsPayable = result.unwrap();

        // Get the amount from the notes.
        let sa = record.notes.replace('$', "").replace(',', "");
        record.amount = sa.parse::<f32>().unwrap();
        record.notes = "".to_string();

        // Make sure we have a transaction id.
        if record.confirmation_number.is_empty() {
            println!("transaction_id is missing: {:?}", record);
            // We don't want to save it to our database.
            continue;
        }

        // Try to link to the correct vendor.
        let vendor = clean_vendor_name(&record.vendor);
        // Try to find the merchant in our list of vendors.
        match SoftwareVendor::get_from_db(&db, vendor.to_string()) {
            Some(v) => {
                record.link_to_vendor = vec![v.airtable_record_id.to_string()];
            }
            None => {
                println!("could not find vendor that matches {}", vendor);
            }
        }

        record.cio_company_id = company.id;

        // Let's add the record to our database.
        record.upsert(&db).await;
    }
}

pub async fn sync_quickbooks(db: &Database, company: &Company) {
    // Authenticate QuickBooks.
    let qb = company.authenticate_quickbooks(&db).await;

    let bill_payments = qb.list_bill_payments().await.unwrap();
    for bill_payment in bill_payments {
        // Let's check if there are any attachments.
        let attachments = qb.list_attachments_for_bill_payment(&bill_payment.id).await.unwrap();

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
            .first::<AccountsPayable>(&db.conn())
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
                            let bill = qb.get_bill(&txn.txn_id).await.unwrap();
                            // Get the attachments for the bill.
                            let attachments = qb.list_attachments_for_bill(&bill.id).await.unwrap();
                            for attachment in attachments {
                                transaction.invoices.push(attachment.temp_download_uri.to_string());
                            }
                        }
                    }
                }
                transaction.cio_company_id = company.id;

                transaction.update(&db).await;
                continue;
            }
            Err(e) => {
                println!("bill payment: {:?}", bill_payment);
                println!(
                    "WARN: could not find transaction with merchant_name `{}` -> `{}` amount `{}` date `{}`: {}",
                    bill_payment.vendor_ref.name, merchant_name, bill_payment.total_amt, bill_payment.txn_date, e
                );
            }
        }
    }

    let purchases = qb.list_purchases().await.unwrap();
    for purchase in purchases.clone() {
        // Let's try to match the Brex reciepts to the transactions.
        if purchase.account_ref.name == "Credit Cards:Brex" {
            // See if we even have attachments.
            let attachments = qb.list_attachments_for_purchase(&purchase.id).await.unwrap();
            if attachments.is_empty() {
                // We can continue early since we don't have attachments.
                continue;
            }

            // This is a brex transaction, let's try to find it in our database to update it.
            // We know we have attachments as well.
            let time_start = NaiveTime::from_hms_milli(0, 0, 0, 0);
            let sdt = purchase.txn_date.checked_sub_signed(Duration::days(10)).unwrap().and_time(time_start);
            let time_end = NaiveTime::from_hms_milli(23, 59, 59, 59);
            let edt = purchase.txn_date.and_time(time_end);
            let merchant_name = clean_merchant_name(&purchase.entity_ref.name);
            match credit_card_transactions::dsl::credit_card_transactions
                .filter(
                    credit_card_transactions::dsl::merchant_name
                        .eq(merchant_name.to_string())
                        .and(credit_card_transactions::dsl::card_vendor.eq("Brex".to_string()))
                        .and(credit_card_transactions::dsl::amount.eq(purchase.total_amt))
                        .and(credit_card_transactions::dsl::time.ge(DateTime::<Utc>::from_utc(sdt, Utc)))
                        .and(credit_card_transactions::dsl::time.le(DateTime::<Utc>::from_utc(edt, Utc))),
                )
                .first::<CreditCardTransaction>(&db.conn())
            {
                Ok(mut transaction) => {
                    // Add the receipt.
                    // Clear out existing receipts.
                    transaction.receipts = vec![];
                    for attachment in attachments {
                        transaction.receipts.push(attachment.temp_download_uri.to_string());
                    }
                    transaction.update(&db).await;
                    continue;
                }
                Err(e) => {
                    println!(
                        "WARN: could not find transaction with merchant_name `{}` -> `{}` amount `{}` date `{}` --> less than `{}` greater than `{}`: {}",
                        purchase.entity_ref.name, merchant_name, purchase.total_amt, purchase.txn_date, sdt, edt, e
                    );
                }
            }
        } else {
            println!("got transaction type: {}", purchase.account_ref.name);
        }
    }
    println!("len: {}", purchases.len());
}

fn clean_merchant_name(s: &str) -> String {
    if s == "Rudys Cant Fail Cafe" {
        "Rudy's Can't Fail Cafe".to_string()
    } else if s == "IKEA" {
        "Ikea".to_string()
    } else if s == "Zoomus" {
        "Zoom.us".to_string()
    } else if s == "MailChimp" {
        "Mailchimp".to_string()
    } else if s == "PAYPAL QUICKLUTION QU" {
        "PAYPAL *QUICKLUTION QU".to_string()
    } else if s == "PCISIG" {
        "PCI-SIG".to_string()
    } else if s == "PAYPAL PC ENGINES" {
        "PAYPAL *PC ENGINES".to_string()
    } else if s == "Paypal Transaction  Eventjarcom Eb" {
        "Paypal Transaction - Eventjarcom Eb".to_string()
    } else if s == "IEEE SA  Products  Services" {
        "IEEE SA - Products & Services".to_string()
    } else if s == "Zeit" {
        "ZEIT".to_string()
    } else if s == "FSCOM  Fiberstore" {
        "FS.COM - Fiberstore".to_string()
    } else if s == "keychroncom" {
        "keychron.com".to_string()
    } else if s == "DURO ENTERPRISE" {
        "Duro".to_string()
    } else if s == "PITCHCOM" {
        "PITCH.COM".to_string()
    } else if s == "SP  CHELSIO WEB STORE" {
        "Chelsio Communications".to_string()
    } else if s == "Temicom" {
        "Temi.com".to_string()
    } else if s == "Ubiquity Global Services Inc" {
        "Ubiquiti Inc.".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::companies::Company;
    use crate::db::Database;
    use crate::finance::{
        refresh_accounts_payable, refresh_bill_com_transactions, refresh_brex_transactions, refresh_expensify_transactions, refresh_ramp_transactions, refresh_software_vendors, sync_quickbooks,
    };

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_finance_quickbooks() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        sync_quickbooks(&db, &oxide).await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_finance_bill_com() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_bill_com_transactions(&db, &oxide).await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_finance_expensify() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_expensify_transactions(&db, &oxide).await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_finance_brex() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_brex_transactions(&db, &oxide).await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_finance() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_software_vendors(&db, &oxide).await;

        refresh_accounts_payable(&db, &oxide).await;

        refresh_ramp_transactions(&db, &oxide).await;
    }
}
