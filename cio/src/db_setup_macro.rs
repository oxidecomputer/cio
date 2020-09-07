extern crate proc_macro;
use proc_macro2::Ident;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_attribute]
pub fn db_setup(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input: DeriveInput = parse_macro_input!(input as DeriveInput);
    let new_struct_names =
        Punctuated::<Ident, Token![,]>::parse_separated_nonempty
            .parse(attr)
            .expect("expected comma separated list of identifiers");
    match &input.data {
        Data::Struct(_) => {
            let all_new_structs: Vec<_> = new_struct_names
                .into_iter()
                .map(|new_struct_name| {
                    let mut new_struct: DeriveInput = input.clone();
                    new_struct.ident = new_struct_name;
                    quote!(#new_struct)
                })
                .collect();
            let expanded = quote!(
                #( #all_new_structs )*
            );
            proc_macro::TokenStream::from(expanded)
        }
        _ => panic!("expected struct"),
    }
}
