use std::collections::BTreeMap;

extern crate proc_macro;

use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;
use serde_tokenstream::from_tokenstream;
use syn::{Field, ItemStruct, Type};

/// The parameters passed to our macro.
#[derive(Deserialize, Debug)]
struct Params {
    /// The name of the new struct that has the added fields of:
    ///   - id: i32
    ///   - airtable_record_id: String
    new_struct_name: String,
    /// The name of the table in Airtable where this information should be sync on every
    /// database operation.
    airtable_table: String,
    /// The Airtable base where this information should be sync on every
    /// database operation.
    airtable_base: String,
    /// A boolean representing if the new struct has a custom PartialEq implementation.
    /// If so, we will not add the derive method PartialEq to the new struct.
    #[serde(default)]
    custom_partial_eq: bool,
    /// The struct item and type that we will filter on to find unique database entries.
    match_on: BTreeMap<String, String>,
}

#[proc_macro_attribute]
pub fn db(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    do_db(attr.into(), item.into()).into()
}

fn do_db(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Get the data from the parameters.
    let params = from_tokenstream::<Params>(&attr).unwrap();

    // Get the names of the new structs.
    let new_struct_name = format_ident!("{}", params.new_struct_name);
    // We also get the name of the new struct in it's plural form so we can generate
    // a type that represents a vector of all the records.
    let new_struct_name_plural = format_ident!("{}s", params.new_struct_name);

    // The `new_struct_name_plural` but in snake_case is the same as our database schema.
    let mut db_schema = format_ident!("{}s", params.new_struct_name.to_snake_case());
    // Special case: RFD -> If the new struct name is all uppercase we need to convert it to
    // lowercase. Otherwise we would have the schema as
    // being `r_f_ds` versus `rfds`.
    if new_struct_name == params.new_struct_name.to_uppercase() {
        db_schema = format_ident!("{}s", params.new_struct_name.to_lowercase());
    }

    // Let's create the database filter.
    let mut filter = quote!();
    let mut args = quote!();
    let mut function_args = quote!();
    for (field, type_) in params.match_on {
        let f = format_ident!("{}", field);
        let t: Type = syn::parse_str(&type_).unwrap();
        filter = quote!(#filter.filter(#db_schema::dsl::#f.eq(#f.clone())));
        args = quote!(#args,#f: #t);
        function_args = quote!(#function_args self.#f.clone(),);
    }

    // Get the original struct information.
    let og_struct: ItemStruct = syn::parse2(item.clone()).unwrap();
    let mut fields: Vec<&Field> = Default::default();
    let mut struct_inners = quote!();
    for field in og_struct.fields.iter() {
        fields.push(field);
        let ident = field.ident.clone();
        struct_inners = quote!(#struct_inners#ident: item.#ident.clone(),);
    }
    let og_struct_name = og_struct.ident;

    // Get the Airtable information.
    let airtable_base = format_ident!("airtable_base_id_{}", params.airtable_base);
    let airtable_table = format_ident!("{}", params.airtable_table);

    let airtable = quote! {
    // Import what we need from diesel so the database queries work.
    use diesel::prelude::*;

    impl #og_struct_name {
        /// Create a new record in the database and Airtable.
        pub async fn create(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
            let mut new_record = self.create_in_db(db)?;

            // Let's also create this record in Airtable.
            let new_airtable_record = new_record.create_in_airtable(db).await?;

            // Now we have the id we need to update the database.
            new_record.airtable_record_id = new_airtable_record.id.to_string();
            let r = new_record.update_in_db(db);
            Ok(r)
        }

        /// Create a new record in the database.
        pub fn create_in_db(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
            // // TODO: special error here.
            let r = diesel::insert_into(crate::schema::#db_schema::table)
                .values(self)
                .get_result(&db.conn())?;

            Ok(r)
        }

        /// Create or update the record in the database and Airtable.
        pub async fn upsert(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
            let mut record = self.upsert_in_db(db)?;

            // Let's also update this record in Airtable.
            let new_airtable_record = record.upsert_in_airtable(db).await?;

            if record.airtable_record_id.is_empty(){
                // Now we have the id we need to update the database.
                record.airtable_record_id = new_airtable_record.id.to_string();
                return Ok(record.update_in_db(db));
            }

            Ok(record)
        }

        /// Create or update the record in the database.
        pub fn upsert_in_db(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
            // See if we already have the record in the database.
            if let Some(r) = #new_struct_name::get_from_db(db, #function_args) {
                // Update the record.
                // // TODO: special error here.
                let record = diesel::update(&r)
                    .set(self)
                    .get_result::<#new_struct_name>(&db.conn())?;

                return Ok(record);
            }

            let r = self.create_in_db(db)?;

            Ok(r)
        }

        /// Get the company object for a record.
        pub fn company(&self, db: &crate::db::Database) -> anyhow::Result<crate::companies::Company> {
            Ok(crate::companies::Company::get_by_id(db, self.cio_company_id)?)
        }
    }

    impl From<#new_struct_name> for #og_struct_name {
        fn from(item: #new_struct_name) -> Self {
            #og_struct_name {
                #struct_inners
            }
        }
    }

    impl #new_struct_name {
        /// Update the record in the database and Airtable.
        pub async fn update(&self, db: &crate::db::Database) -> anyhow::Result<Self> {
            // Update the record.
            let mut record = self.update_in_db(db);

            // Let's also update this record in Airtable.
            let new_airtable_record = record.upsert_in_airtable(db).await?;

            // Now we have the id we need to update the database.
            record.airtable_record_id = new_airtable_record.id.to_string();
            Ok(record.update_in_db(db))
        }

        /// Update the record in the database.
        pub fn update_in_db(&self, db: &crate::db::Database) -> Self {
            // Update the record.
            diesel::update(self)
                .set(self.clone())
                .get_result::<#new_struct_name>(&db.conn())
                .unwrap_or_else(|e| panic!("[db] unable to update record {}: {}", self.id, e))
        }

        /// Get a record from the database.
        pub fn get_from_db(db: &crate::db::Database#args) -> Option<Self> {
            match #db_schema::dsl::#db_schema#filter.first::<#new_struct_name>(&db.conn()) {
                Ok(r) => {
                    return Some(r);
                }
                Err(e) => {
                    println!("[db] we don't have the record in the database: {}", e);
                    return None;
                }
            }
        }

        /// Get a record by its id.
        pub fn get_by_id(db: &crate::db::Database, id: i32) -> anyhow::Result<Self> {
            let record = #db_schema::dsl::#db_schema.find(id)
                .first::<#new_struct_name>(&db.conn())?;

            Ok(record)
        }

        /// Get the company object for a record.
        pub fn company(&self, db: &crate::db::Database) -> anyhow::Result<crate::companies::Company> {
            Ok(crate::companies::Company::get_by_id(db, self.cio_company_id)?)
        }

        /// Get the row in our airtable workspace.
        pub async fn get_from_airtable(id: &str, db: &crate::db::Database, cio_company_id: i32) -> anyhow::Result<Self> {
            let record = #new_struct_name::airtable_from_company_id(db, cio_company_id)?
                .get_record(&#new_struct_name::airtable_table(), id)
                .await?;

            Ok(record.fields)
        }

        /// Delete a record from the database and Airtable.
        pub async fn delete(&self, db: &crate::db::Database) -> anyhow::Result<()> {
            self.delete_from_db(db)?;

            // Let's also delete the record from Airtable.
            self.delete_from_airtable(db).await?;

            Ok(())
        }

        /// Delete a record from the database.
        pub fn delete_from_db(&self, db: &crate::db::Database) -> anyhow::Result<()> {
            diesel::delete(
                crate::schema::#db_schema::dsl::#db_schema.filter(
                    crate::schema::#db_schema::dsl::id.eq(self.id)))
                    .execute(&db.conn())?;

            Ok(())
        }

        /// Create the Airtable client.
        /// We do this in it's own function so our other functions are more DRY.
        fn airtable(&self, db: &crate::db::Database) -> anyhow::Result<airtable_api::Airtable> {
            // Get the company for the company_id.
            let company = self.company(db)?;
            Ok(company.authenticate_airtable(&company.#airtable_base))
        }

        /// Create the Airtable client.
        /// We do this in it's own function so our other functions are more DRY.
        fn airtable_from_company_id(db: &crate::db::Database, cio_company_id: i32) -> anyhow::Result<airtable_api::Airtable> {
            // Get the company for the company_id.
            let company = crate::companies::Company::get_by_id(db, cio_company_id)?;
            Ok(company.authenticate_airtable(&company.#airtable_base))
        }

        /// Return the Airtable table name.
        /// We do this in it's own function so our other functions are more DRY.
        fn airtable_table() -> String {
            #airtable_table.to_string()
        }

        /// Create the row in the Airtable base.
        pub async fn create_in_airtable(&mut self, db: &crate::db::Database) -> anyhow::Result<airtable_api::Record<#new_struct_name>> {
            let mut mut_self = self.clone();
            // Run the custom trait to update the new record from the old record.
            // We do this because where we join Airtable tables, things tend to get a little
            // weird if we aren't nit picky about this.
            mut_self.update_airtable_record(self.clone()).await;

            // Create the record.
            let record = airtable_api::Record {
                id: "".to_string(),
                created_time: None,
                fields: mut_self,
            };

            // Send the new record to the Airtable client.
            let records : Vec<airtable_api::Record<#new_struct_name>> = self.airtable(db)?
                .create_records(&#new_struct_name::airtable_table(), vec![record])
                .await
                ?;

            println!("[airtable] created new row: {:?}", self);

            // Return the first record back.
            Ok(records.get(0).unwrap().clone())
        }

        /// Update the record in Airtable.
        pub async fn update_in_airtable(&self, db: &crate::db::Database, existing_record: &mut airtable_api::Record<#new_struct_name>) -> anyhow::Result<airtable_api::Record<#new_struct_name>> {
            let mut mut_self = self.clone();
            // Run the custom trait to update the new record from the old record.
            // We do this because where we join Airtable tables, things tend to get a little
            // weird if we aren't nit picky about this.
            mut_self.update_airtable_record(existing_record.fields.clone()).await;

            // If the Airtable record and the record that was passed in are the same, then we can return early since
            // we do not need to update it in Airtable.
            // We do this after we update the record so that any fields that are links to other
            // tables match as well and this can return true even if we have linked records.
            if mut_self == existing_record.fields {
                println!("[airtable] id={} in given object equals Airtable record, skipping update", self.id);
                return Ok(existing_record.clone());
            }

            existing_record.fields = mut_self;

            // Send the updated record to Airtable.
            let records : Vec<airtable_api::Record<#new_struct_name>> = self.airtable(db)?.update_records(
                &#new_struct_name::airtable_table(),
                vec![existing_record.clone()],
            ).await?;

            println!("[airtable] id={} updated", self.id);

            if records.is_empty() {
                return Ok(existing_record.clone());
            }

            Ok(records.get(0).unwrap().clone())
        }

        /// Get the existing record in Airtable that matches this id.
        pub async fn get_existing_airtable_record(&self, db: &crate::db::Database) -> Option<airtable_api::Record<#new_struct_name>> {
            if self.airtable_record_id.is_empty() {
                return None;
            }
                // Let's get the existing record from airtable.
                if let Ok(a) = self.airtable(db) {
                        match a.get_record(&#new_struct_name::airtable_table(), &self.airtable_record_id)
                        .await {
                            Ok(v) => return Some(v),
                            Err(e) => {
                                println!("getting airtable record failed: {}", self.airtable_record_id);
                                return None;
                            }
                        }
                }

            None
        }


        /// Create or update a row in the Airtable base.
        pub async fn upsert_in_airtable(&mut self, db: &crate::db::Database) -> anyhow::Result<airtable_api::Record<#new_struct_name>> {
            // First check if we have an `airtable_record_id` for this record.
            // If we do we can move ahead faster.
            if !self.airtable_record_id.is_empty() {
                let mut er: Option<airtable_api::Record<#new_struct_name>> = self.get_existing_airtable_record(db).await;

                if let Some(mut existing_record) = er {
                    // Return the result from the update.
                    return Ok(self.update_in_airtable(db, &mut existing_record).await?);
                }
                // Otherwise we need to continue through the other loop.
            }

            // Since we don't know the airtable record id, we need to find it by looking
            // through all the existing records in Airtable and matching on our database id.
            // This is slow so we should always try to make sure we have the airtable_record_id
            // set. This function is mostly here until we migrate away from the old way of doing
            // things.
            let records = #new_struct_name_plural::get_from_airtable(db,self.cio_company_id).await?;
            for (id, record) in records {
                if self.id == id {
                    return Ok(self.update_in_airtable(db, &mut record.clone()).await?);
                }
            }

            // We've tried everything to find the record in our existing Airtable but it is not
            // there. We need to create it.
            let record = self.create_in_airtable(db).await?;

            Ok(record)
        }

        /// Delete a record from Airtable.
        pub async fn delete_from_airtable(&self, db: &crate::db::Database) -> anyhow::Result<()> {
            if !self.airtable_record_id.is_empty() {
                // Delete the record from airtable.
                self.airtable(db)?.delete_record(&#new_struct_name::airtable_table(), &self.airtable_record_id).await?;
            }

            Ok(())
        }
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct #new_struct_name_plural(pub Vec<#new_struct_name>);

    impl IntoIterator for #new_struct_name_plural {
        type Item = #new_struct_name;
        type IntoIter = std::vec::IntoIter<Self::Item>;

        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl From<#new_struct_name_plural> for Vec<#new_struct_name> {
        fn from(item: #new_struct_name_plural) -> Self {
            item.0
        }
    }

    impl #new_struct_name_plural {
        /// Get the current records for this type from the database.
        pub fn get_from_db(db: &crate::db::Database, cio_company_id: i32) -> anyhow::Result<Self> {
            Ok(#new_struct_name_plural(
                crate::schema::#db_schema::dsl::#db_schema
                    .filter(crate::schema::#db_schema::dsl::cio_company_id.eq(cio_company_id))
                    .order_by(crate::schema::#db_schema::dsl::id.desc())
                    .load::<#new_struct_name>(&db.conn())?
            ))
        }

        /// Get the current records for this type from Airtable.
        pub async fn get_from_airtable(db: &crate::db::Database, cio_company_id: i32) -> anyhow::Result<std::collections::BTreeMap<i32, airtable_api::Record<#new_struct_name>>> {
            let result: Vec<airtable_api::Record<#new_struct_name>> = #new_struct_name::airtable_from_company_id(db, cio_company_id)?
                .list_records(&#new_struct_name::airtable_table(), "Grid view", vec![])
                .await?;

            let mut records: std::collections::BTreeMap<i32, airtable_api::Record<#new_struct_name>> =
                Default::default();
            for record in result {
                records.insert(record.fields.id, record);
            }

            Ok(records)
        }

        /// Update Airtable records in a table from a vector.
        pub async fn update_airtable(&self, db: &crate::db::Database) -> anyhow::Result<()> {
            if self.0.is_empty() {
                // Return early.
                return Ok(());
            }
            let mut records = #new_struct_name_plural::get_from_airtable(db, self.0.get(0).unwrap().cio_company_id).await?;

            for mut vec_record in self.0.clone() {
                // See if we have it in our Airtable records.
                match records.get(&vec_record.id) {
                    Some(r) => {
                        let mut record = r.clone();

                        // Update the record in Airtable.
                        vec_record.update_in_airtable(db, &mut record).await?;

                        // Remove it from the map.
                        records.remove(&vec_record.id);
                    }
                    None => {
                        // We do not have the record in Airtable, Let's create it.
                        // Create the record in Airtable.
                        vec_record.create_in_airtable(db).await?;

                        // Remove it from the map.
                        records.remove(&vec_record.id);
                    }
                }
            }

            // Iterate over the records remaining and remove them from airtable
            // since they don't exist in our vector.
            for (_, record) in records {
                // Delete the record from airtable.
                record.fields.airtable(db)?.delete_record(&#new_struct_name::airtable_table(), &record.id).await?;
            }

            Ok(())
        }
    }
    };

    // Does this struct have a custom PartialEq function?
    let mut partial_eq_text = Default::default();
    if !params.custom_partial_eq {
        partial_eq_text = quote!(PartialEq,);
    }

    let new_struct = quote!(
        #item

        #[derive(
            Debug,
            Queryable,
            Identifiable,
            Associations,
            AsChangeset,
            #partial_eq_text
            Clone,
            JsonSchema,
            Deserialize,
            Serialize,
        )]
        pub struct #new_struct_name {
            // This has to be the first field.
            #[serde(default)]
            pub id: i32,
            #(#fields),*,
            // This has to be the last field, due to the schemas.
            #[serde(default, skip_serializing_if = "String::is_empty")]
            pub airtable_record_id: String,
        }

        #airtable
    );
    new_struct
}
