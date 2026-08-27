mod csrf;
mod handlers;
pub(crate) mod password;
mod session;

pub(crate) use csrf::verify_same_origin;
pub(crate) use handlers::dispatch_public;
pub(crate) use session::{AdminIdentity, authenticate, now};

#[derive(Debug, Clone)]
pub struct AuthSource(pub String);

pub(crate) fn source(parts: &http::request::Parts) -> &str {
    parts
        .extensions
        .get::<AuthSource>()
        .map(|source| source.0.as_str())
        .unwrap_or("unknown")
}
