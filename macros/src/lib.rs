#![deny(clippy::all)]
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
    // An optional target struct to implement against. This is used to limit interactions
    // between this macro and other macros that may generate new structs
    target_struct: Option<String>,
    /// The name of the new struct that has the added fields of:
    ///   - id: i32
    ///   - airtable_record_id: String
    new_struct_name: String,
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

    // Get the original struct information.
    let og_struct: ItemStruct = syn::parse2(item.clone()).unwrap();

    if params.target_struct.is_none() || og_struct.ident == params.target_struct.unwrap() {
        // Get the names of the new structs.
        let new_struct_name = format_ident!("{}", params.new_struct_name);
        let new_struct_name_str = &params.new_struct_name;

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

        let mut fields: Vec<&Field> = Default::default();
        let mut struct_inners = quote!();
        for field in og_struct.fields.iter() {
            fields.push(field);
            let ident = field.ident.clone();
            struct_inners = quote!(#struct_inners #ident: item.#ident.clone(),);
        }
        let og_struct_name = og_struct.ident;

        let db_impl = quote! {
        // Import what we need from diesel so the database queries work.
        use diesel::prelude::*;

        impl #og_struct_name {
            /// Create a new record in the database and Airtable.
            pub async fn create(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
                let r = diesel::insert_into(crate::schema::#db_schema::table)
                    .values(self.clone())
                    .get_result_async(db.pool()).await?;

                Ok(r)
            }

            /// Create or update the record in the database and Airtable.
            pub async fn upsert(&self, db: &crate::db::Database) -> anyhow::Result<#new_struct_name> {
                log::info!("Upserting {} record", #new_struct_name_str);

                // See if we already have the record in the database.
                if let Some(r) = #new_struct_name::get_from_db(db, #function_args).await {
                    // Update the record.
                    // TODO: special error here.

                    log::info!("Found existing {} record. Performing update", #new_struct_name_str);
                    let record = diesel::update(#db_schema::dsl::#db_schema)
                        .filter(#db_schema::dsl::id.eq(r.id))
                        .set(self.clone())
                        .get_result_async::<#new_struct_name>(db.pool()).await?;

                    return Ok(record);
                }

                log::info!("No existing {} record. Performing create", #new_struct_name_str);

                let r = self.create(db).await?;

                Ok(r)
            }

            /// Get the company object for a record.
            pub async fn company(&self, db: &crate::db::Database) -> anyhow::Result<crate::companies::Company> {
                match crate::companies::Company::get_by_id(db, self.cio_company_id).await {
                    Ok(c) => Ok(c),
                    Err(e) => Err(anyhow::anyhow!("getting company for record `{:?}` failed: {}", self, e))
                }
            }
        }

        impl From<#new_struct_name> for #og_struct_name {
            fn from(item: #new_struct_name) -> Self {
                #og_struct_name {
                    #struct_inners
                }
            }
        }

        impl From<&#new_struct_name> for #og_struct_name {
            fn from(item: &#new_struct_name) -> Self {
                #og_struct_name {
                    #struct_inners
                }
            }
        }

        impl #new_struct_name {
            /// Update the record in the database and Airtable.
            pub async fn update(&self, db: &crate::db::Database) -> anyhow::Result<Self> {
                // Update the record.
                let record = diesel::update(#db_schema::dsl::#db_schema)
                    .filter(#db_schema::dsl::id.eq(self.id))
                    .set(self.clone())
                    .get_result_async::<#new_struct_name>(db.pool()).await?;

                Ok(record)
            }

            /// Get a record from the database.
            pub async fn get_from_db(db: &crate::db::Database #args) -> Option<Self> {
                match #db_schema::dsl::#db_schema #filter.first_async::<#new_struct_name>(db.pool()).await {
                    Ok(r) => {
                        return Some(r);
                    }
                    Err(e) => {
                        log::debug!("[db] we don't have the record in the database: {}", e);
                        return None;
                    }
                }
            }

            /// Get a record by its id.
            pub async fn get_by_id(db: &crate::db::Database, id: i32) -> anyhow::Result<Self> {
                let record = #db_schema::dsl::#db_schema.find(id)
                    .first_async::<#new_struct_name>(db.pool()).await?;

                Ok(record)
            }

            /// Get a record by its airtable id.
            pub async fn get_by_airtable_id(db: &crate::db::Database, id: &str) -> anyhow::Result<Self> {
                let record = #db_schema::dsl::#db_schema.filter(#db_schema::dsl::airtable_record_id.eq(id.to_string()))
                    .first_async::<#new_struct_name>(db.pool()).await?;

                Ok(record)
            }

            /// Get the company object for a record.
            pub async fn company(&self, db: &crate::db::Database) -> anyhow::Result<crate::companies::Company> {
                match crate::companies::Company::get_by_id(db, self.cio_company_id).await {
                    Ok(c) => Ok(c),
                    Err(e) => Err(anyhow::anyhow!("getting company for record `{:?}` failed: {}", self, e))
                }
            }

            /// Delete a record from the database and Airtable.
            pub async fn delete(&self, db: &crate::db::Database) -> anyhow::Result<()> {
                diesel::delete(
                    crate::schema::#db_schema::dsl::#db_schema.filter(
                        crate::schema::#db_schema::dsl::id.eq(self.id)))
                        .execute_async(db.pool()).await?;

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
            pub async fn get_from_db(db: &crate::db::Database, cio_company_id: i32) -> anyhow::Result<Self> {
                match
                    crate::schema::#db_schema::dsl::#db_schema
                        .filter(crate::schema::#db_schema::dsl::cio_company_id.eq(cio_company_id))
                        .order_by(crate::schema::#db_schema::dsl::id.desc())
                        .load_async::<#new_struct_name>(db.pool()).await
                {
                    Ok(r) => Ok(#new_struct_name_plural(r)),
                    Err(e) => Err(anyhow::anyhow!("getting `{:?}` from the database for cio_company_id `{}` failed: {}", #new_struct_name_plural(vec![]), cio_company_id, e)),
                }
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

            #db_impl
        );

        new_struct
    } else {
        quote! {
            #item
        }
    }
}
