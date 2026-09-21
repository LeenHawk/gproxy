//! Builders for protocol structs with required arguments and optional setters.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, parse_macro_input};

mod declared_fields;

/// Rebuilds the same wire type using declared fields, recursively dropping
/// extension bags without reading, cloning, or serializing them. Mark a bag
/// with `#[declared(extension)]`; `#[serde(flatten)] rest` is also recognized.
/// Formal JSON fields are preserved. This derive does not change serde behavior.
/// An alternate protocol crate path can be supplied with `#[declared(crate = "path")]`.
/// `#[declared(bound = "T: path::DeclaredFields")]` replaces inferred formal-field
/// bounds, for example for mutually recursive generic types. Existing where
/// clauses and extension-field Default bounds are retained.
#[proc_macro_derive(DeclaredFields, attributes(declared))]
pub fn declared_fields(input: TokenStream) -> TokenStream {
    declared_fields::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Generates `Type::builder(required_fields...)` and a `TypeBuilder`.
///
/// Required fields are supplied in declaration order. `Option<T>` fields start
/// as `None` and have setters accepting `T`; `rest` starts empty and has a
/// setter accepting its declared type. `build()` returns the completed value.
/// Mark a nullable but required field with `#[wire(required)]` to require an
/// explicit `Option<T>` argument instead of defaulting it in the builder.
#[proc_macro_derive(WireBuilder, attributes(wire))]
pub fn wire_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "WireBuilder requires a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input,
            "WireBuilder requires named fields",
        ));
    };
    let name = &input.ident;
    let builder = format_ident!("{}Builder", name);
    let visibility = &input.vis;
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut arguments = Vec::new();
    let mut initializers = Vec::new();
    let mut setters = Vec::new();

    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let mut required = false;
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("wire"))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("required") {
                    required = true;
                    Ok(())
                } else {
                    Err(meta.error("expected `required`"))
                }
            })?;
        }
        if required {
            arguments.push(quote!(#ident: #ty));
            initializers.push(quote!(#ident));
            continue;
        }
        if let Some(inner) = option_inner(ty) {
            initializers.push(quote!(#ident: ::core::option::Option::None));
            setters.push(quote! {
                #[must_use]
                pub fn #ident(mut self, value: impl ::core::convert::Into<#inner>) -> Self {
                    self.0.#ident = ::core::option::Option::Some(value.into());
                    self
                }
            });
        } else if ident == "rest" {
            initializers.push(quote!(#ident: ::core::default::Default::default()));
            setters.push(quote! {
                #[must_use]
                pub fn #ident(mut self, value: #ty) -> Self {
                    self.0.#ident = value;
                    self
                }
            });
        } else {
            arguments.push(quote!(#ident: #ty));
            initializers.push(quote!(#ident));
        }
    }

    Ok(quote! {
        impl #impl_generics #name #type_generics #where_clause {
            /// Start a builder by supplying the required fields in declaration order.
            #[allow(clippy::too_many_arguments)]
            pub fn builder(#(#arguments),*) -> #builder #type_generics {
                #builder(Self { #(#initializers),* })
            }
        }

        #[doc = concat!("Builder for [`", stringify!(#name), "`].")]
        #[must_use]
        #visibility struct #builder #generics (#name #type_generics) #where_clause;

        impl #impl_generics #builder #type_generics #where_clause {
            #(#setters)*

            /// Return the completed wire value.
            pub fn build(self) -> #name #type_generics {
                self.0
            }
        }
    })
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}
