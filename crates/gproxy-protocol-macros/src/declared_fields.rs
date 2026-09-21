use proc_macro2::{TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Generics, Path, parse::Parser, parse_quote};

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let mut root: Path = parse_quote!(::gproxy_protocol);
    let mut bounds = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("declared"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                root = meta.value()?.parse::<syn::LitStr>()?.parse()?;
                Ok(())
            } else if meta.path.is_ident("bound") {
                let value: syn::LitStr = meta.value()?.parse()?;
                bounds = Some(
                    syn::punctuated::Punctuated::<syn::WherePredicate, syn::Token![,]>::parse_terminated
                        .parse_str(&value.value())?,
                );
                Ok(())
            } else {
                Err(meta.error("expected `crate = \"path\"` or `bound = \"predicates\"` on a type"))
            }
        })?;
    }
    let trait_path: Path = parse_quote!(#root::wire::DeclaredFields);
    let mut generics = input.generics.clone();
    let infer_bounds = bounds.is_none();
    if let Some(bounds) = bounds {
        generics.make_where_clause().predicates.extend(bounds);
    }
    let body = match &input.data {
        Data::Struct(data) => {
            let (pattern, value) = fields(
                &data.fields,
                &trait_path,
                &input.ident,
                infer_bounds,
                &mut generics,
            )?;
            quote! { let Self #pattern = self; Self #value }
        }
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                let name = &variant.ident;
                let attrs = variant.attrs.iter().filter(|a| a.path().is_ident("cfg"));
                let (pattern, value) = fields(
                    &variant.fields,
                    &trait_path,
                    &input.ident,
                    infer_bounds,
                    &mut generics,
                )?;
                arms.push(quote! { #(#attrs)* Self::#name #pattern => Self::#name #value });
            }
            quote! { match self { #(#arms),* } }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input,
                "DeclaredFields does not support unions",
            ));
        }
    };
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #trait_path for #name #type_generics #where_clause {
            fn into_declared(self) -> Self { #body }
        }
    })
}

fn fields(
    fields: &Fields,
    trait_path: &Path,
    type_name: &syn::Ident,
    infer_bounds: bool,
    generics: &mut Generics,
) -> syn::Result<(TokenStream, TokenStream)> {
    let mut patterns = Vec::new();
    let mut values = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let mut extension = false;
        let mut flattened = false;
        for attr in &field.attrs {
            if attr.path().is_ident("declared") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("extension") {
                        extension = true;
                        Ok(())
                    } else {
                        Err(meta.error("expected `extension` on a field"))
                    }
                })?;
            }
            if attr.path().is_ident("serde") {
                // Parse syntactically, without interpreting unrelated serde options.
                let metas = attr.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )?;
                flattened |= metas
                    .iter()
                    .any(|meta| matches!(meta, syn::Meta::Path(p) if p.is_ident("flatten")));
            }
        }
        extension |= flattened && field.ident.as_ref().is_some_and(|name| name == "rest");
        let ty = &field.ty;
        let binding = format_ident!("__declared_field_{index}");
        let pattern = if extension {
            quote!(_)
        } else {
            quote!(#binding)
        };
        let value = if extension {
            if generics
                .type_params()
                .any(|p| mentions(quote!(#ty), &p.ident))
            {
                generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#ty: ::core::default::Default));
            }
            quote!(::core::default::Default::default())
        } else {
            let parameters: Vec<_> = generics
                .type_params()
                .filter(|p| mentions(quote!(#ty), &p.ident))
                .map(|p| p.ident.clone())
                .collect();
            // A bound on a recursive field creates a trait solver cycle.
            // Its nonrecursive fields supply the necessary parameter bounds.
            // Other fields need bounds on their actual type: T::Item and
            // PhantomData<T> do not imply that T implements this trait.
            if infer_bounds
                && !parameters.is_empty()
                && !mentions(quote!(#ty), type_name)
                && !mentions(quote!(#ty), &format_ident!("Self"))
            {
                generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#ty: #trait_path));
            }
            quote!(#trait_path::into_declared(#binding))
        };
        let attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();
        if let Some(name) = &field.ident {
            patterns.push(quote!(#(#attrs)* #name: #pattern));
            values.push(quote!(#(#attrs)* #name: #value));
        } else {
            patterns.push(quote!(#(#attrs)* #pattern));
            values.push(quote!(#(#attrs)* #value));
        }
    }
    Ok(match fields {
        Fields::Named(_) => (quote!({ #(#patterns),* }), quote!({ #(#values),* })),
        Fields::Unnamed(_) => (quote!(( #(#patterns),* )), quote!(( #(#values),* ))),
        Fields::Unit => (quote!(), quote!()),
    })
}

fn mentions(tokens: TokenStream, name: &syn::Ident) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == *name,
        TokenTree::Group(group) => mentions(group.stream(), name),
        _ => false,
    })
}
