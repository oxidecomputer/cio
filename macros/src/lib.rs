extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;
use serde_tokenstream::from_tokenstream;
use serde_tokenstream::Error;
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
pub fn db_struct(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match do_db_struct(attr.into(), item.into()) {
        Ok(result) => result.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn do_db_struct(
    attr: TokenStream,
    item: TokenStream,
) -> Result<TokenStream, Error> {
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
            pub async fn push_to_airtable(&self) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), #base_id);

                // Create the record.
                let record = airtable_api::Record {
                    id: None,
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
        }

        pub struct #new_name_plural(pub Vec<#new_name>);
        impl #new_name_plural {
            /// Update Airtable records in a table from a vector.
            pub async fn update_airtable(&self) {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    #base_id,
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

                for mut vec_record in self.0.clone() {
                    // See if we have it in our Airtable records.
                    match records.get(&vec_record.id) {
                        Some(r) => {
                            // If the Airtable record and the vector record are the same, then we can return early since
                            // we do not need to update it in Airtable.
                            if vec_record == r.fields {
                                println!("[airtable] id={} in vector equals Airtable record, skipping update", vec_record.id);
                                continue;
                            }

                            // Let's update the Airtable record with the record from the vector.
                            let mut record = r.clone();

                            // Run the custom trait to update the new record from the old record.
                            vec_record.update_airtable_record(record.fields.clone());

                            record.fields = vec_record.clone();

                            airtable
                                .update_records(
                                    #table,
                                    vec![record.clone()],
                                )
                                .await
                                .unwrap();
                            println!(
                                "[airtable] id={} updated in Airtable",
                                vec_record.id
                            );
                        }
                        None => {
                            // We do not have the record in Airtable, let's create it.
                            // Create the record in Airtable.
                            vec_record.push_to_airtable().await;
                            println!(
                                "[airtable] id={} created in Airtable",
                                vec_record.id
                            );
                        }
                    }
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
    Ok(new_struct)
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
            pub async fn push_to_airtable(&self) {
                // Initialize the Airtable client.
                let airtable =
                    airtable_api::Airtable::new(airtable_api::api_key_from_env(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

                // Create the record.
                let record = airtable_api::Record {
                    id: None,
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
        }

        pub struct DuplicatedItems(pub Vec<DuplicatedItem>);
        impl DuplicatedItems {
            /// Update Airtable records in a table from a vector.
            pub async fn update_airtable(&self) {
                // Initialize the Airtable client.
                let airtable = airtable_api::Airtable::new(
                    airtable_api::api_key_from_env(),
                    AIRTABLE_BASE_ID_CUSTOMER_LEADS,
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

                for mut vec_record in self.0.clone() {
                    // See if we have it in our Airtable records.
                    match records.get(&vec_record.id) {
                        Some(r) => {
                            // If the Airtable record and the vector record are the same, then we can return early since
                            // we do not need to update it in Airtable.
                            if vec_record == r.fields {
                                println!("[airtable] id={} in vector equals Airtable record, skipping update", vec_record.id);
                                continue;
                            }

                            // Let's update the Airtable record with the record from the vector.
                            let mut record = r.clone();

                            // Run the custom trait to update the new record from the old record.
                            vec_record.update_airtable_record(record.fields.clone());

                            record.fields = vec_record.clone();

                            airtable
                                .update_records(
                                    AIRTABLE_RFD_TABLE,
                                    vec![record.clone()],
                                )
                                .await
                                .unwrap();
                            println!(
                                "[airtable] id={} updated in Airtable",
                                vec_record.id
                            );
                        }
                        None => {
                            // We do not have the record in Airtable, let's create it.
                            // Create the record in Airtable.
                            vec_record.push_to_airtable().await;
                            println!(
                                "[airtable] id={} created in Airtable",
                                vec_record.id
                            );
                        }
                    }
                }
            }
        }
        };

        assert_eq!(expected.to_string(), ret.unwrap().to_string());
    }
}
