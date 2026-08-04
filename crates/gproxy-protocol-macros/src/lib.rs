//! Derives and expression helpers used by `gproxy-protocol`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::{self, VisitMut};
use syn::{Data, DeriveInput, Expr, ExprStruct, Fields, Meta, Type, parse_macro_input};

#[proc_macro_derive(WireBuilder, attributes(serde))]
pub fn derive_wire_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let builder = format_ident!("Wire{name}Builder");
    let generics = input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let Data::Struct(data) = input.data else {
        return syn::Error::new_spanned(name, "WireBuilder only supports structs")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = data.fields else {
        return syn::Error::new_spanned(name, "WireBuilder requires named fields")
            .to_compile_error()
            .into();
    };

    let fields: Vec<_> = fields.named.into_iter().collect();
    let names: Vec<_> = fields
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect();
    let types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let values = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        if field_has_default(field) {
            quote! { self.#field_name.unwrap_or_default() }
        } else {
            quote! {
                self.#field_name.ok_or_else(|| crate::WireBuildError::missing(
                    stringify!(#name),
                    stringify!(#field_name),
                ))?
            }
        }
    });

    quote! {
        #[doc(hidden)]
        pub struct #builder #generics {
            #(#names: ::core::option::Option<#types>,)*
        }

        impl #impl_generics #builder #type_generics #where_clause {
            #(
                pub fn #names(mut self, value: #types) -> Self {
                    self.#names = ::core::option::Option::Some(value);
                    self
                }
            )*

            pub fn build(self) -> ::core::result::Result<#name #type_generics, crate::WireBuildError> {
                ::core::result::Result::Ok(#name {
                    #(#names: #values,)*
                })
            }
        }

        impl #impl_generics #name #type_generics #where_clause {
            pub fn builder() -> #builder #type_generics {
                #builder {
                    #(#names: ::core::option::Option::None,)*
                }
            }
        }
    }
    .into()
}

fn field_has_default(field: &syn::Field) -> bool {
    if matches!(
        &field.ty,
        Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "Option")
    ) {
        return true;
    }
    field.attrs.iter().any(|attribute| {
        if !attribute.path().is_ident("serde") {
            return false;
        }
        let Meta::List(list) = &attribute.meta else {
            return false;
        };
        let tokens = list.tokens.to_string();
        tokens.split(',').any(|part| {
            let part = part.trim();
            part == "default"
                || part.starts_with("default =")
                || part == "skip"
                || part == "skip_deserializing"
        })
    })
}

/// Construct a non-exhaustive wire struct through its generated builder while
/// retaining familiar named-field syntax at transform implementation sites.
#[proc_macro]
pub fn wire(input: TokenStream) -> TokenStream {
    let expression = parse_macro_input!(input as ExprStruct);
    expand_struct(expression).into()
}

struct NestedWireRewriter;

impl VisitMut for NestedWireRewriter {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        visit_mut::visit_expr_mut(self, expression);
        let Expr::Struct(struct_expression) = expression else {
            return;
        };
        if !looks_like_enum_variant(&struct_expression.path) {
            *expression = Expr::Verbatim(expand_struct(struct_expression.clone()));
        }
    }
}

fn looks_like_enum_variant(path: &syn::Path) -> bool {
    path.segments
        .iter()
        .rev()
        .nth(1)
        .and_then(|segment| segment.ident.to_string().chars().next())
        .is_some_and(char::is_uppercase)
}

fn expand_struct(mut expression: ExprStruct) -> proc_macro2::TokenStream {
    let mut rewriter = NestedWireRewriter;
    for field in &mut expression.fields {
        rewriter.visit_expr_mut(&mut field.expr);
    }
    if let Some(rest) = expression.rest.as_mut() {
        rewriter.visit_expr_mut(rest);
    }
    if looks_like_enum_variant(&expression.path) {
        return quote! { #expression };
    }
    let path = expression.path;
    let fields = expression.fields;

    if let Some(rest) = expression.rest {
        let assignments = fields.iter().map(|field| {
            let member = &field.member;
            let value = &field.expr;
            quote! { value.#member = #value; }
        });
        return quote! {{
            let mut value: #path = #rest;
            #(#assignments)*
            value
        }};
    }

    let setters = fields.iter().map(|field| {
        let member = &field.member;
        let value = &field.expr;
        quote! { .#member(#value) }
    });
    quote! {{
        #path::builder()
            #(#setters)*
            .build()
            .expect(concat!("complete ", stringify!(#path), " wire construction"))
    }}
}
