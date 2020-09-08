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
    table: Option<String>,
    base_id: Option<String>,
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

    let old_struct: ItemStruct = syn::parse2(item.clone()).unwrap();
    let mut fields: Vec<&Field> = Default::default();
    for field in old_struct.fields.iter() {
        fields.push(field);
    }

    let mut airtable = Default::default();
    if metadata.base_id.is_some() && metadata.table.is_some() {
        let base_id = format_ident!("{}", metadata.base_id.unwrap());
        let table = format_ident!("{}", metadata.table.unwrap());

        airtable = quote!(
        impl #new_name {
            /// Push the row to our Airtable workspace.
            pub async fn push_to_airtable(&self) {
                // Initialize the Airtable client.
                let airtable =
                    Airtable::new(airtable_api_key(), #base_id);

                // Create the record.
                let record = Record {
                    id: None,
                    created_time: None,
                    fields: serde_json::to_value(self).unwrap(),
                };

                // Send the new record to the Airtable client.
                // Batch can only handle 10 at a time.
                airtable
                    .create_records(#table, vec![record])
                    .await
                    .unwrap();

                println!("created new row in airtable: {:?}", self);
            }
        }
            );
    }

    let new_struct = quote!(
        #item

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
                        Airtable::new(airtable_api_key(), AIRTABLE_BASE_ID_CUSTOMER_LEADS);

                    // Create the record.
                    let record = Record {
                        id: None,
                        created_time: None,
                        fields: serde_json::to_value(self).unwrap(),
                    };

                    // Send the new record to the Airtable client.
                    // Batch can only handle 10 at a time.
                    airtable
                        .create_records(AIRTABLE_RFD_TABLE, vec![record])
                        .await
                        .unwrap();

                    println!("created new row in airtable: {:?}", self);
                }
            }
        };

        assert_eq!(expected.to_string(), ret.unwrap().to_string());
    }
}
