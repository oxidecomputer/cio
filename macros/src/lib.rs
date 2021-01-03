extern crate proc_macro;

use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;
use serde_tokenstream::from_tokenstream;
use syn::{Field, ItemStruct};

#[derive(Deserialize, Debug)]
struct Metadata {
    new_name: String,
    #[serde(default)]
    table: String,
    #[serde(default)]
    base_id: String,
    #[serde(default)]
    custom_partial_eq: bool,
    #[serde(default)]
    airtable_fields: Vec<String>,
}

#[proc_macro_attribute]
pub fn db_struct(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    do_db_struct(attr.into(), item.into()).into()
}

fn do_db_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
    let metadata = from_tokenstream::<Metadata>(&attr).unwrap();
    let new_name = format_ident!("{}", metadata.new_name);
    let new_name_plural = format_ident!("{}s", metadata.new_name);

    let old_struct: ItemStruct = syn::parse2(item.clone()).unwrap();
    let mut fields: Vec<&Field> = Default::default();
    for field in old_struct.fields.iter() {
        fields.push(field);
    }

    let mut airtable = Default::default();
    if !metadata.base_id.is_empty() && !metadata.table.is_empty() {
        let base_id = format_ident!("{}", metadata.base_id);
        let table = format_ident!("{}", metadata.table);
        let airtable_fields = metadata.airtable_fields;

        airtable = quote!(
        impl #new_name {
            /// Push the row to our Airtable workspace.
            #[tracing::instrument]
            #[inline]
            pub async fn push_to_airtable(&self) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), #base_id, "");

                // Create the record.
                let record = airtable_api::Record {
                    id: "".to_string(),
                    created_time: None,
                    fields: self.clone(),
                };

                // Send the new record to the Airtable client.
                // Batch can only handle 10 at a time.
                let _ : Vec<airtable_api::Record<#new_name>> = airtable
                    .create_records(#table, vec![record])
                    .await
                    .unwrap();

                println!("created new row in airtable: {:?}", self);
            }

            /// Update the record in airtable.
            #[tracing::instrument]
            #[inline]
            pub async fn update_in_airtable(&mut self, existing_record: &mut airtable_api::Record<#new_name>) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), #base_id, "");

                // Run the custom trait to update the new record from the old record.
                self.update_airtable_record(existing_record.fields.clone()).await;

                // If the Airtable record and the record that was passed in are the same, then we can return early since
                // we do not need to update it in Airtable.
                // We do this after we update the record so that those fields match as
                // well.
                if self.clone() == existing_record.fields.clone() {
                    println!("[airtable] id={} in given object equals Airtable record, skipping update", self.id);
                    return;
                }

                existing_record.fields = self.clone();

                airtable
                    .update_records(
                        #table,
                        vec![existing_record.clone()],
                    )
                    .await
                    .unwrap();
                println!(
                    "[airtable] id={} updated in Airtable",
                    self.id
                );
            }

            /// Update a row in our airtable workspace.
            #[tracing::instrument]
            #[inline]
            pub async fn create_or_update_in_airtable(&mut self) {
                // Check if we already have the row in Airtable.
                let records = #new_name_plural::get_from_airtable().await;
                for (id, record) in records {
                    if self.id == id {
                        self.update_in_airtable(&mut record.clone()).await;

                        return;
                    }
                }

                // The record does not exist. We need to create it.
                self.push_to_airtable().await;
            }
        }

        pub struct #new_name_plural(pub Vec<#new_name>);
        impl #new_name_plural {
            /// Get the current records from Airtable.
            #[tracing::instrument]
            #[inline]
            pub async fn get_from_airtable() -> std::collections::BTreeMap<i32, airtable_api::Record<#new_name>> {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    #base_id,
                    "",
                );

                let result: Vec<airtable_api::Record<#new_name>> = airtable
                    .list_records(#table, "Grid view", vec![#(#airtable_fields),*])
                    .await
                    .unwrap();

                let mut records: std::collections::BTreeMap<i32, airtable_api::Record<#new_name>> =
                    Default::default();
                for record in result {
                    records.insert(record.fields.id, record);
                }

                records
            }

            /// Update Airtable records in a table from a vector.
            #[tracing::instrument(skip(self))]
            #[inline]
            pub async fn update_airtable(&self) {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    #base_id,
                    "",
                );

                let mut records = #new_name_plural::get_from_airtable().await;

                for mut vec_record in self.0.clone() {
                    // See if we have it in our Airtable records.
                    match records.get(&vec_record.id) {
                        Some(r) => {
                            vec_record.update_in_airtable(&mut r.clone()).await;

                            // Remove it from the map.
                            records.remove(&vec_record.id);
                        }
                        None => {
                            // We do not have the record in Airtable, let's create it.
                            // Create the record in Airtable.
                            vec_record.push_to_airtable().await;
                            println!(
                                "[airtable] id={} created in Airtable",
                                vec_record.id
                            );

                            // Remove it from the map.
                            records.remove(&vec_record.id);
                        }
                    }
                }

                // Iterate over the records remaining and remove them from airtable
                // since they don't exist in our vector.
                for (_, record) in records {
                    // Delete the record from airtable.
                    airtable.delete_record(#table, &record.id).await.unwrap();
                }
            }
        }
                    );
    }

    // Does this struct have a custom PartialEq function?
    let mut partial_eq_text = Default::default();
    if !metadata.custom_partial_eq {
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
        pub struct #new_name {
            pub id: i32,
            #(#fields),*
        }

        #airtable
    );
    new_struct
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_struct() {
        let ret = do_db_struct(
            quote! {
                new_name = DuplicatedItem,
                base_id = AIRTABLE_BASE_ID_CUSTOMER_LEADS,
                table = AIRTABLE_RFD_TABLE,
            }
            .into(),
            quote! {
                pub struct Item {
                    pub foo: String,
                    pub bar: String
                }
            }
            .into(),
        );
        let expected = quote! {
        pub struct Item {
            pub foo: String,
            pub bar: String
        }

        #[derive(
            Debug,
            Queryable,
            Identifiable,
            Associations,
            AsChangeset,
            PartialEq,
            Clone,
            JsonSchema,
            Deserialize,
            Serialize,
        )]
        pub struct DuplicatedItem {
            pub id: i32,
            pub foo: String,
            pub bar: String
        }

        impl DuplicatedItem {
            /// Push the row to our Airtable workspace.
            #[tracing::instrument]
            #[inline]
            pub async fn push_to_airtable(&self) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_CUSTOMER_LEADS, "");

                // Create the record.
                let record = airtable_api::Record {
                    id: "".to_string(),
                    created_time: None,
                    fields: self.clone(),
                };

                // Send the new record to the Airtable client.
                // Batch can only handle 10 at a time.
                let _ : Vec<airtable_api::Record<DuplicatedItem>> = airtable
                    .create_records(AIRTABLE_RFD_TABLE, vec![record])
                    .await
                    .unwrap();

                println!("created new row in airtable: {:?}", self);
            }

            /// Update the record in airtable.
            #[tracing::instrument]
            #[inline]
            pub async fn update_in_airtable(&mut self, existing_record: &mut airtable_api::Record<DuplicatedItem>) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_CUSTOMER_LEADS, "");

                // Run the custom trait to update the new record from the old record.
                self.update_airtable_record(existing_record.fields.clone()).await;

                // If the Airtable record and the record that was passed in are the same, then we can return early since
                // we do not need to update it in Airtable.
                // We do this after we update the record so that those fields match as
                // well.
                if self.clone() == existing_record.fields.clone() {
                    println!("[airtable] id={} in given object equals Airtable record, skipping update", self.id);
                    return;
                }

                existing_record.fields = self.clone();

                airtable
                    .update_records(
                        AIRTABLE_RFD_TABLE,
                        vec![existing_record.clone()],
                    )
                    .await
                    .unwrap();
                println!(
                    "[airtable] id={} updated in Airtable",
                    self.id
                );
            }

            /// Update a row in our airtable workspace.
            #[tracing::instrument]
            #[inline]
            pub async fn create_or_update_in_airtable(&mut self) {
                // Check if we already have the row in Airtable.
                let records = DuplicatedItems::get_from_airtable().await;
                for (id, record) in records {
                    if self.id == id {
                        self.update_in_airtable(&mut record.clone()).await;

                        return;
                    }
                }

                // The record does not exist. We need to create it.
                self.push_to_airtable().await;
            }
        }

        pub struct DuplicatedItems(pub Vec<DuplicatedItem>);
        impl DuplicatedItems {
            /// Get the current records from Airtable.
            #[tracing::instrument]
            #[inline]
            pub async fn get_from_airtable() -> std::collections::BTreeMap<i32, airtable_api::Record<DuplicatedItem>> {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    AIRTABLE_BASE_ID_CUSTOMER_LEADS,
                    "",
                );

                let result: Vec<airtable_api::Record<DuplicatedItem>> = airtable
                    .list_records(AIRTABLE_RFD_TABLE, "Grid view", vec![])
                    .await
                    .unwrap();

                let mut records: std::collections::BTreeMap<i32, airtable_api::Record<DuplicatedItem>> =
                    Default::default();
                for record in result {
                    records.insert(record.fields.id, record);
                }

                records
            }

            /// Update Airtable records in a table from a vector.
            #[tracing::instrument(skip(self))]
            #[inline]
            pub async fn update_airtable(&self) {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    AIRTABLE_BASE_ID_CUSTOMER_LEADS,
                    "",
                );

                let mut records = DuplicatedItems::get_from_airtable().await;

                for mut vec_record in self.0.clone() {
                    // See if we have it in our Airtable records.
                    match records.get(&vec_record.id) {
                        Some(r) => {
                            vec_record.update_in_airtable(&mut r.clone()).await;

                            // Remove it from the map.
                            records.remove(&vec_record.id);
                        }
                        None => {
                            // We do not have the record in Airtable, let's create it.
                            // Create the record in Airtable.
                            vec_record.push_to_airtable().await;
                            println!(
                                "[airtable] id={} created in Airtable",
                                vec_record.id
                            );

                            // Remove it from the map.
                            records.remove(&vec_record.id);
                        }
                    }
                }

                // Iterate over the records remaining and remove them from airtable
                // since they don't exist in our vector.
                for (_, record) in records {
                    // Delete the record from airtable.
                    airtable.delete_record(AIRTABLE_RFD_TABLE, &record.id).await.unwrap();
                }
            }
        }
        };

        assert_eq!(expected.to_string(), ret.to_string());
    }
}

/* OUR NEW MACRO */

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
    /// The Airtable base ID where this information should be sync on every
    /// database operation.
    airtable_base_id: String,
    /// A boolean representing if the new struct has a custom PartialEq implementation.
    /// If so, we will not add the derive method PartialEq to the new struct.
    #[serde(default)]
    custom_partial_eq: bool,
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

    // Get the original struct information.
    let og_struct: ItemStruct = syn::parse2(item.clone()).unwrap();
    let mut fields: Vec<&Field> = Default::default();
    for field in og_struct.fields.iter() {
        fields.push(field);
    }

    // Get the Airtable information.
    let airtable_base_id = format_ident!("{}", params.airtable_base_id);
    let airtable_table = format_ident!("{}", params.airtable_table);

    let airtable = quote!(
    // Import what we need from diesel so the database queries work.
    use diesel::prelude::*;

    impl #new_struct_name {
        /// Update the record in the database.
        #[instrument(skip(db))]
        #[inline]
        pub fn update_in_db(&self, db: crate::db::Database) -> Self {
            // Update the record.
            diesel::update(self)
                .set(self.clone())
                .get_result::<#new_struct_name>(&db.conn())
                .unwrap_or_else(|e| panic!("[db] unable to update record {}: {}", self.id, e))
        }

        /// Create the Airtable client.
        /// We do this in it's own function so our other functions are more DRY.
        #[tracing::instrument]
        #[inline]
        fn airtable() -> airtable_api::Airtable {
            airtable_api::Airtable::new(airtable_api::api_key_from_env(), #airtable_base_id, "")
        }

        /// Return the Airtable table name.
        /// We do this in it's own function so our other functions are more DRY.
        #[tracing::instrument]
        #[inline]
        fn airtable_table() -> String {
            #airtable_table.to_string()
        }

        /// Create the row in the Airtable base.
        #[tracing::instrument]
        #[inline]
        pub async fn create_in_airtable(&mut self) {
            // Create the record.
            let record = airtable_api::Record {
                id: "".to_string(),
                created_time: None,
                fields: self.clone(),
            };

            // Send the new record to the Airtable client.
            let records : Vec<airtable_api::Record<#new_struct_name>> = #new_struct_name::airtable()
                .create_records(&#new_struct_name::airtable_table(), vec![record])
                .await
                .unwrap();

            println!("[airtable] created new row: {:?}", self);

            // Get the first record back.
            let new_record = records.get(0).unwrap();

            // Now we have the id we need to update the database.
            self.airtable_record_id = new_record.id.to_string();
            // TODO: we should use our pool of connections.
            self.update_in_db(crate::db::Database::new());
        }

        /// Update the record in Airtable.
        #[tracing::instrument]
        #[inline]
        pub async fn update_in_airtable(&mut self, existing_record: &mut airtable_api::Record<#new_struct_name>) {
            // Run the custom trait to update the new record from the old record.
            // We do this because where we join Airtable tables, things tend to get a little
            // weird if we aren't nit picky about this.
            self.update_airtable_record(existing_record.fields.clone()).await;

            // If we do not have an airtable_record_id set in the database for this record, we are
            // going to want to set it.
            // TODO: Eventually we can remove this logic, when everything is migrated.
            if self.airtable_record_id.is_empty() {
                self.airtable_record_id = existing_record.id.to_string();
                // Now let's update the database.
                // TODO: use our pool of connections.
                self.update_in_db(crate::db::Database::new());
                // Now we know in the future that we will have the right airtable_record_id and can
                // update more easily.
            }

            // If the Airtable record and the record that was passed in are the same, then we can return early since
            // we do not need to update it in Airtable.
            // We do this after we update the record so that any fields that are links to other
            // tables match as well and this can return true even if we have linked records.
            if self == existing_record.fields {
                println!("[airtable] id={} in given object equals Airtable record, skipping update", self.id);
                return;
            }

            existing_record.fields = self.clone();

            // Send the updated record to Airtable.
            #new_struct_name::airtable().update_records(
                &#new_struct_name::airtable_table(),
                vec![existing_record.clone()],
            ).await.unwrap();

            println!("[airtable] id={} updated", self.id);
        }

        /// Get the existing record in Airtable that matches this id.
        #[tracing::instrument]
        #[inline]
        pub async fn get_existing_airtable_record(&self) -> airtable_api::Record<#new_struct_name> {
                // Let's get the existing record from airtable.
                #new_struct_name::airtable()
                    .get_record(&#new_struct_name::airtable_table(), &self.airtable_record_id)
                    .await.unwrap()
        }


        /// Update a row in our airtable workspace.
        #[tracing::instrument]
        #[inline]
        pub async fn create_or_update_in_airtable(&mut self) {
            // First check if we have an `airtable_record_id` for this record.
            // If we do we can move ahead faster.
            if !self.airtable_record_id.is_empty() {
                let mut existing_record: airtable_api::Record<#new_struct_name> = self.get_existing_airtable_record().await;

                // Return the result from the update.
                return self.update_in_airtable(&mut existing_record).await;
            }

            // Since we don't know the airtable record id, we need to find it by looking
            // through all the existing records in Airtable and matching on our database id.
            // This is slow so we should always try to make sure we have the airtable_record_id
            // set. This function is mostly here until we migrate away from the old way of doing
            // things.
            let records = #new_struct_name_plural::get_from_airtable().await;
            for (id, record) in records {
                if self.id == id {
                    self.update_in_airtable(&mut record.clone()).await;

                    return;
                }
            }

            // We've tried everything to find the record in our existing Airtable but it is not
            // there. We need to create it.
            self.create_in_airtable().await;
        }
    }

    pub struct #new_struct_name_plural(pub Vec<#new_struct_name>);
    impl #new_struct_name_plural {
        /// Get the current records for this type from Airtable.
        #[tracing::instrument]
        #[inline]
        pub async fn get_from_airtable() -> std::collections::BTreeMap<i32, airtable_api::Record<#new_struct_name>> {
            let result: Vec<airtable_api::Record<#new_struct_name>> = #new_struct_name::airtable()
                .list_records(&#new_struct_name::airtable_table(), "Grid view", vec![])
                .await
                .unwrap();

            let mut records: std::collections::BTreeMap<i32, airtable_api::Record<#new_struct_name>> =
                Default::default();
            for record in result {
                records.insert(record.fields.id, record);
            }

            records
        }

        /// Get the current records for this type from the database.
        #[tracing::instrument(skip(db))]
        #[inline]
        pub fn get_from_db(db: crate::db::Database) -> Self {
            #new_struct_name_plural(
                crate::schema::#db_schema::dsl::#db_schema
                    .order_by(crate::schema::#db_schema::dsl::id.desc())
                    .load::<#new_struct_name>(&db.conn())
                    .unwrap()
            )
        }

        /// Update Airtable records in a table from a vector.
        #[tracing::instrument(skip(self))]
        #[inline]
        pub async fn update_airtable(&self) {
            let mut records = #new_struct_name_plural::get_from_airtable().await;

            for mut vec_record in self.0.clone() {
                // See if we have it in our Airtable records.
                match records.get(&vec_record.id) {
                    Some(r) => {
                        let mut record = r.clone();

                        // Update the record in Airtable.
                        vec_record.update_in_airtable(&mut record).await;

                        // Remove it from the map.
                        records.remove(&vec_record.id);
                    }
                    None => {
                        // We do not have the record in Airtable, Let's create it.
                        // Create the record in Airtable.
                        vec_record.create_in_airtable().await;

                        // Remove it from the map.
                        records.remove(&vec_record.id);
                    }
                }
            }

            // Iterate over the records remaining and remove them from airtable
            // since they don't exist in our vector.
            for (_, record) in records {
                // Delete the record from airtable.
                #new_struct_name::airtable().delete_record(&#new_struct_name::airtable_table(), &record.id).await.unwrap();
            }
        }
    }
                );

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

/*
        /// Get a record from the database.
        #[tracing::instrument]
        #[inline]
        pub async fn get_from_db(db: crate::db::Database, #db_match_on: #db_match_on_type) -> Option<Self> {
            match #db_schema::dsl::#db_schema.filter(#db_schema::dsl::#db_match_on.eq(#db_match_on)).limit(1).load::<#new_struct_name>(&db.conn()) {
                Ok(r) => {
                    if !r.is_empty() {
                        return Some(r.get(0).unwrap().clone());
                    }
                }
                Err(e) => {
                    println!("[db] we don't have the record `{:?}` in the database: {}", #db_match_on, e);
                    return None;
                }
            }

            None
        }
*/
