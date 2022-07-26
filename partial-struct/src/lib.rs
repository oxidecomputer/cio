#![allow(dead_code, unused_imports)]

extern crate proc_macro;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Group, Span};
use quote::ToTokens;
use syn::{
    bracketed,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input,
    punctuated::Punctuated,
    AttributeArgs, DeriveInput, Ident, LitStr, Result, Token,
};

/// Optional commands that can be define on a per field level. Commands are paired with the
/// specific struct name that it applies to.
///
/// Commands:
///   skip - Omits the field from the newly generated struct
#[derive(Debug)]
struct FieldCommands {
    name: Ident,
    skip: bool,
}

/// Currently field command syntax is a comma separated list of values. This will likely be
/// replaced with a more flexible syntax in the future
impl Parse for FieldCommands {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;

        let content;
        syn::parenthesized!(content in input);

        // Invalid or unknown commands are currently ignored. They likely will be upgraded to
        // to errors in the future
        let commands: Punctuated<Ident, Token![,]> = content.parse_terminated(Ident::parse)?;

        let skip = commands.iter().any(|c| c == "skip");

        Ok(FieldCommands { name, skip })
    }
}

/// A holder of derive identifiers to add or remove from generated structs
#[derive(Debug, Clone)]
struct DeriveOptions {
    traits: Vec<Ident>,
}

impl Parse for DeriveOptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut traits = vec![];

        while !input.is_empty() {
            let t: Ident = input.parse()?;

            // We do not try to validate the derive, if an invalid derive is supplied then it
            // will fail during compilation later on
            traits.push(t);

            // Parse an optional comma following each trait
            let _: Result<Token![,]> = input.parse();
        }

        Ok(DeriveOptions { traits })
    }
}

/// A parsed out struct that has been requested to be created
#[derive(Debug)]
struct NewStruct {
    name: Ident,

    // Additional derives that should be added to the struct
    with: Option<DeriveOptions>,

    // Derives that should be removed from the new struct (if they exist)
    without: Option<DeriveOptions>,
}

impl Parse for NewStruct {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;

        let mut new_struct = NewStruct {
            name,
            with: None,
            without: None,
        };

        while !input.is_empty() {
            // If there are remaining tokens then the next token must be a comma
            let _: Token![,] = input.parse()?;

            // Following there are two possible options `with` and `without`
            let option: Ident = input.parse()?;

            if option == "with" {
                let to_add: Group = input.parse()?;
                let tokens = to_add.stream();
                let traits: DeriveOptions = syn::parse2(tokens)?;
                new_struct.with = Some(traits);
            } else if option == "without" {
                let to_add: Group = input.parse()?;
                let tokens = to_add.stream();
                let traits: DeriveOptions = syn::parse2(tokens)?;
                new_struct.without = Some(traits);
            } else {
                return Err(syn::Error::new(option.span(), "unknown option"));
            }
        }

        Ok(new_struct)
    }
}

#[derive(Debug)]
struct Derives {
    derives: Vec<Ident>,
}

impl Parse for Derives {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::parenthesized!(content in input);

        let mut derives: Vec<Ident> = vec![];

        while !content.is_empty() {
            let derive: Ident = content.parse()?;
            let _: Result<Token![,]> = content.parse();

            derives.push(derive);
        }

        Ok(Derives { derives })
    }
}

fn compute_derives(
    mut attr: syn::Attribute,
    with: Option<DeriveOptions>,
    without: Option<DeriveOptions>,
) -> syn::Attribute {
    let derives: Result<Derives> = syn::parse2(attr.tokens);
    let mut derive_list = derives.map(|d| d.derives).unwrap_or_else(|_| vec![]);

    if let Some(mut with) = with {
        derive_list.append(&mut with.traits);
    }

    if let Some(without) = without {
        derive_list.retain(|item| !without.traits.contains(item));
    }

    let stream = quote! {
        (#( #derive_list ),*)
    };

    attr.tokens = stream;

    attr
}

/// A macro (for structs) that generates a new struct containing a subset of the fields of the
/// tagged struct. New structs can have additional derive values added or removed. Any subsequent
/// (non-partial) derives or macros will be applied to the new structs as well.
///
/// Examples:
///
/// ```
/// // Using all of the macro features
///
/// #[partial(NewStruct, with(Debug), without(Default))]
/// #[derive(Default)]
/// struct OldStruct {
///    a: u32
///    #[partial(NewStruct(skip))]
///    b: u32
/// }
///
/// // will generate
///
/// #[derive(Debug)]
/// struct NewStruct {
///    a: u32
/// }
/// ```
#[proc_macro_attribute]
pub fn partial(attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // We will need this name later when generating [From] impls
    let original_name = input.ident;

    // Parse the attribute that triggered this macro to execute
    let first_new_struct = parse_macro_input!(attr as NewStruct);

    // Look through the remaining attributes on this struct and find any other instances of the
    // [partial] macro. For each of those, parse them into their own [NewStruct]
    let additional_structs: Result<Vec<NewStruct>> = input
        .attrs
        .iter()
        .filter_map(|attr| {
            if attr.path.is_ident("partial") {
                Some(attr.parse_args())
            } else {
                None
            }
        })
        .collect();

    match additional_structs {
        Ok(mut additional_structs) => {

            // Construct the full list of structs that need to be created. This list needs to
            // include the original struct (without modification) as well
            let mut new_structs = vec![
                NewStruct {
                    name: original_name.clone(),
                    with: None,
                    without: None,
                },
                first_new_struct,
            ];
            new_structs.append(&mut additional_structs);

            let result = match input.data {

                // This macro is only defined for structs. Usage on any other data type will
                // fail with a panic
                syn::Data::Struct(ref s) => {
                    let visibility = input.vis;
                    let generics = input.generics;

                    // Create the list of all attribute macros that need to be applied to the
                    // generated structs
                    let attr_without_partials: Vec<&syn::Attribute> = input
                        .attrs
                        .iter()
                        .filter(|attr| !attr.path.is_ident("partial"))
                        .collect();

                    // From the list of attributes, find all of the non-derive attributes
                    let struct_attrs: Vec<&syn::Attribute> = attr_without_partials
                        .into_iter()
                        .filter(|attr| !attr.path.is_ident("derive"))
                        .collect();

                    // Find the derive attribute if one exists
                    let orig_derives: Option<&syn::Attribute> =
                        input.attrs.iter().filter(|attr| attr.path.is_ident("derive")).nth(0);

                    // Keep track of all of the structs to output
                    let mut expanded_structs = vec![];

                    if let syn::Fields::Named(ref fields) = s.fields {

                        // Generate each of the requested structs
                        for new_struct in new_structs {
                            let NewStruct { name, with, without } = new_struct;

                            // Generate the list of fields to assign to the new struct
                            let filtered_fields: Vec<syn::Field> = fields
                                .named
                                .iter()
                                // Omit any fields with the `partial` attribute that include the
                                // `skip` command
                                .filter(|field| {
                                    !field.attrs.iter().any(|attr| {
                                        if attr.path.is_ident("partial") {
                                            
                                            // This ideally would be a helpful error message instead
                                            // of a panic
                                            let parsed: FieldCommands =
                                                attr.parse_args().expect("Failed to parse field args");
                                            parsed.name == name && parsed.skip
                                        } else {
                                            false
                                        }
                                    })
                                })
                                .map(|field| {
                                    let mut field = field.to_owned();

                                    // Filter out any `partial` attributes assigned to the field
                                    field.attrs = field
                                        .attrs
                                        .into_iter()
                                        .filter(|attr| !attr.path.is_ident("partial"))
                                        .collect();

                                    field
                                })
                                .collect();

                            // If this is not the original struct being generated, create a default
                            // From impl from the original struct to the new struct.
                            let from_impl = if name != original_name {
                                let field_names = filtered_fields
                                    .iter()
                                    .map(|field| field.ident.as_ref())
                                    .collect::<Vec<Option<&Ident>>>();

                                quote! {
                                    impl #generics From<#original_name #generics> for #name #generics {
                                        fn from(orig: #original_name #generics) -> Self {
                                            Self {
                                                #( #field_names: orig.#field_names, )*
                                            }
                                        }
                                    }
                                }
                            } else {
                                quote! {}
                            };

                            let derives: Option<syn::Attribute> = orig_derives.map(|d| d.to_owned());

                            let derive_attr = if let Some(derives) = derives {

                                // Add in and/or remove the additional derives defined by the caller.
                                // Adding derives may result in further compilation errors in the
                                // fields in the original struct are not compatible with the new
                                // derives
                                let computed = compute_derives(derives, with, without);
                                quote! { #computed }
                            } else {
                                quote! {}
                            };

                            expanded_structs.push(quote! {
                                #derive_attr
                                #( #struct_attrs )*
                                #visibility struct #name #generics {
                                    #( #filtered_fields, )*
                                }

                                #from_impl
                            });
                        }
                    }

                    proc_macro::TokenStream::from(quote! {
                        #( #expanded_structs )*
                    })
                }

                // Ideally this would return a descriptive error instead of panicking
                other => panic!(
                    "Partial can only be defined on structs. Attempted to define on {:#?}",
                    other
                ),
            };

            result
        }
        Err(err) => err.to_compile_error().into(),
    }
}
