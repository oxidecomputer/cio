extern crate proc_macro;

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
                    airtable.delete_record(#table, &record.id).await;
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
                    airtable.delete_record(AIRTABLE_RFD_TABLE, &record.id).await;
                }
            }
        }
        };

        assert_eq!(expected.to_string(), ret.to_string());
    }
}
