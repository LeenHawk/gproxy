mod csrf;
mod handlers;
pub(crate) mod password;
pub(crate) mod session;

pub(crate) use csrf::verify_same_origin;
pub(crate) use handlers::dispatch_public;
pub(crate) use session::{AdminIdentity, authenticate, now};

#[derive(Debug, Clone)]
pub struct AuthSource(pub String);

pub(crate) fn source(parts: &http::request::Parts) -> &str {
    source_ip(parts).unwrap_or("unknown")
}

pub(crate) fn source_ip(parts: &http::request::Parts) -> Option<&str> {
    parts
        .extensions
        .get::<AuthSource>()
        .map(|source| source.0.as_str())
}
