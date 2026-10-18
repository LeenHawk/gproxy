//! Check documentation coverage against actual Rust route declarations.
//! This only parses syntax; it never opens a host or executes a handler.
use std::{collections::BTreeSet, fs, path::PathBuf};
use syn::{
    Expr, ItemFn, ItemMod, Lit, Token,
    parse::Parser,
    punctuated::Punctuated,
    visit::{self, Visit},
};

pub fn inventory() {
    let mut routes = BTreeSet::new();
    for entry in fs::read_dir("src/admin").unwrap() {
        let path = entry.unwrap().path();
        // The font router, like its DTO, exists only on native hosts.
        if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32"
            && path.file_name().is_some_and(|name| name == "fonts.rs")
        {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs")
            && !path.to_string_lossy().ends_with("_docs.rs")
        {
            collect(&path, "/admin/api", &mut routes);
        }
    }
    collect(&PathBuf::from("src/portal.rs"), "/portal/api", &mut routes);
    collect(&PathBuf::from("src/update.rs"), "/admin/api", &mut routes);
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("documented-routes.txt");
    fs::write(output, routes.into_iter().collect::<Vec<_>>().join("\n")).unwrap();
}

fn collect(path: &std::path::Path, prefix: &str, routes: &mut BTreeSet<String>) {
    println!("cargo:rerun-if-changed={}", path.display());
    let source = fs::read_to_string(path).unwrap();
    let file = syn::parse_file(&source).unwrap();
    Inventory { prefix, routes }.visit_file(&file);
}

struct Inventory<'a> {
    prefix: &'a str,
    routes: &'a mut BTreeSet<String>,
}
fn excluded(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && match &attr.meta {
                syn::Meta::List(list) => {
                    let text = list.tokens.to_string();
                    text == "test" || text.contains("documentation")
                }
                _ => false,
            }
    })
}
fn path_value(expr: &Expr) -> String {
    let Expr::Lit(expr) = expr else {
        panic!("route path must be a string literal")
    };
    let Lit::Str(value) = &expr.lit else {
        panic!("route path must be a string literal")
    };
    value.value()
}
fn methods(expr: &Expr) -> Vec<String> {
    const METHODS: &[&str] = &[
        "get", "post", "put", "patch", "delete", "head", "options", "trace",
    ];
    match expr {
        Expr::Call(call) => {
            let Expr::Path(path) = call.func.as_ref() else {
                panic!("unsupported route constructor")
            };
            let name = path.path.segments.last().unwrap().ident.to_string();
            assert!(
                METHODS.contains(&name.as_str()),
                "undocumented route constructor: {name}"
            );
            vec![name]
        }
        Expr::MethodCall(call) => {
            let mut result = methods(&call.receiver);
            let name = call.method.to_string();
            if METHODS.contains(&name.as_str()) {
                result.push(name);
            }
            result
        }
        Expr::Macro(expr) => {
            let name = expr.mac.path.segments.last().unwrap().ident.to_string();
            let verbs: &[&str] = match name.as_str() {
                "collection" | "config_collection" => &["get", "post"],
                "batch" => &["post"],
                "item" => &["get", "patch"],
                "config_item" => &["get", "patch", "delete"],
                _ => panic!("undocumented method-router macro: {name}"),
            };
            verbs.iter().map(|verb| (*verb).to_owned()).collect()
        }
        Expr::Paren(expr) => methods(&expr.expr),
        _ => panic!("unsupported method-router expression"),
    }
}
impl Inventory<'_> {
    fn add(&mut self, path: &str, method: &str) {
        self.routes
            .insert(format!("{method} {}{path}", self.prefix));
    }
}
impl<'ast> Visit<'ast> for Inventory<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if !excluded(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !excluded(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "route" {
            assert_eq!(call.args.len(), 2);
            let path = path_value(&call.args[0]);
            for method in methods(&call.args[1]) {
                self.add(&path, &method);
            }
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_macro(&mut self, expr: &'ast syn::ExprMacro) {
        let name = expr.mac.path.segments.last().unwrap().ident.to_string();
        if ["family", "config_family", "scoped_family"].contains(&name.as_str()) {
            let args = Punctuated::<Expr, Token![,]>::parse_terminated
                .parse2(expr.mac.tokens.clone())
                .unwrap();
            let path = path_value(&args[1]);
            for (suffix, verbs) in [
                ("", &["get", "post"][..]),
                ("/batch", &["post"][..]),
                ("/{id}", &["get", "patch", "delete"][..]),
            ] {
                for verb in verbs {
                    self.add(&format!("{path}{suffix}"), verb);
                }
            }
            // Nested router expressions can themselves contain routes/families.
            self.visit_expr(&args[0]);
        }
    }
}
