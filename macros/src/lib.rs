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
}

#[proc_macro_attribute]
pub fn db_setup(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match do_macro(attr.into(), item.into()) {
        Ok(result) => result.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn do_macro(
    attr: TokenStream,
    item: TokenStream,
) -> Result<TokenStream, Error> {
    let metadata = from_tokenstream::<Metadata>(&attr).unwrap();
    let new_name = metadata.new_name;
    let new_name_ident = format_ident!("{}", new_name);

    let old_struct: ItemStruct = syn::parse2(item.clone()).unwrap();
    let mut fields: Vec<&Field> = Default::default();
    for field in old_struct.fields.iter() {
        fields.push(field);
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
        pub struct #new_name_ident {
            pub id: i32,
            #(#fields),*
        }
    );
    Ok(new_struct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_setup() {
        let ret = do_macro(
            quote! {
                new_name = DuplicatedItem,
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
        };

        assert_eq!(expected.to_string(), ret.unwrap().to_string());
    }
}
