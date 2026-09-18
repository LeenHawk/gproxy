//! Operator rewrite rules: compilation at assembly time, application at
//! execution time. Regexes and paths compile once per configuration revision;
//! execution never re-validates rule rows.

mod compile;

pub use compile::{RewriteCompileError, compile_rule};
