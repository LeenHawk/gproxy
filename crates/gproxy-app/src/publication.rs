//! Publication links: the URL a stored body is served from, and the two
//! routes that serve it.
//!
//! Core owns published bytes (an image it generated, a file a conversion
//! spilled to storage) and the id they are recorded under, but it has no
//! public HTTP surface, so it cannot say where they can be fetched from. The
//! host does: it mounts a route, and it implements `PublicationUrl` to name
//! it. `url_for` is called *before* the body is written, so a refusal costs
//! nothing.
//!
//! # Why an unset base refuses instead of guessing
//!
//! `AppConfig.public_base_url` is the one thing this crate cannot infer. A
//! request's `Host` header is chosen by the client, an `x-forwarded-proto` is
//! chosen by whatever is in front, and either can be wrong in a way nobody
//! notices until a caller follows a link. A link built from a guess is worse
//! than no link at all: the upstream answer is accepted, the bytes are stored,
//! the URL is handed to the caller, and it 404s somewhere else. So with no
//! base configured [`AppPublicationUrl`] answers `None`, the publish fails
//! with `Unsupported` before anything is written, and the caller is told to
//! ask for the bytes inline (`b64_json`) instead.

use gproxy_core::{Publication, PublicationRef, PublicationUrl};
use gproxy_seaorm::BatchConnectionTrait;

use crate::{App, AppConfig, AppError};

/// The path segment publications are served under. A host mounting anything
/// else has to implement its own `PublicationUrl`.
pub const PUBLICATION_PATH: &str = "/publications";

/// `{base}/publications/{id}`, or nothing when no base is configured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppPublicationUrl {
    base: String,
}

impl AppPublicationUrl {
    /// A trailing slash on the base is dropped, so `https://x/` and
    /// `https://x` produce the same link rather than `https://x//publications/…`.
    pub fn new(base: impl Into<String>) -> Self {
        let base = base.into();
        Self {
            base: base.trim().trim_end_matches('/').to_owned(),
        }
    }

    /// The instance's external origin, or nothing configured.
    pub fn from_config(config: &AppConfig) -> Self {
        Self::new(config.public_base_url.clone().unwrap_or_default())
    }

    /// Whether this can mint a link at all. A host can check once at startup
    /// and warn, rather than letting every publication fail at request time.
    pub fn is_configured(&self) -> bool {
        !self.base.is_empty()
    }

    pub fn base(&self) -> &str {
        &self.base
    }
}

impl PublicationUrl for AppPublicationUrl {
    fn url_for(&self, publication: &PublicationRef<'_>) -> Option<String> {
        self.is_configured()
            .then(|| format!("{}{PUBLICATION_PATH}/{}", self.base, publication.id))
    }
}

impl<C: BatchConnectionTrait + Send + Sync> App<C> {
    /// The publication behind `id`: its metadata and stored bytes, for the
    /// host's download route.
    ///
    /// `NotFound` when the id names no published body, when the body was
    /// released, and when its expiry has passed — the three are one answer on
    /// purpose, because distinguishing them tells a prober which ids existed.
    /// **The scope is not checked**: the id is a random secret and the link is
    /// the capability, exactly as core's own read has it.
    pub async fn read_publication(&self, id: &str) -> Result<Publication, AppError> {
        self.gproxy()
            .core()
            .read_publication(id)
            .await?
            .ok_or_else(|| AppError::not_found("publication", id))
    }

    /// Tombstone the publication behind `id` and delete its bytes. `NotFound`
    /// when there is nothing published under it, so a repeated delete is a
    /// 404 rather than a silent success.
    pub async fn delete_publication(&self, id: &str) -> Result<(), AppError> {
        self.gproxy()
            .core()
            .delete_publication(id)
            .await?
            .then_some(())
            .ok_or_else(|| AppError::not_found("publication", id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(id: &str) -> PublicationRef<'_> {
        PublicationRef {
            id,
            scope: "user:u1",
            mime: Some("image/png"),
            expires_at_ms: None,
        }
    }

    #[test]
    fn a_configured_base_names_the_hosts_own_route() {
        let builder = AppPublicationUrl::new("https://gproxy.example.com");
        assert_eq!(
            builder.url_for(&reference("pub-1")).as_deref(),
            Some("https://gproxy.example.com/publications/pub-1")
        );
    }

    #[test]
    fn a_trailing_slash_does_not_double_up() {
        let builder = AppPublicationUrl::new("  https://gproxy.example.com/  ");
        assert_eq!(builder.base(), "https://gproxy.example.com");
        assert_eq!(
            builder.url_for(&reference("pub-1")).as_deref(),
            Some("https://gproxy.example.com/publications/pub-1")
        );
    }

    #[test]
    fn no_base_refuses_rather_than_minting_a_dead_link() {
        let builder = AppPublicationUrl::from_config(&AppConfig::default());
        assert!(!builder.is_configured());
        assert_eq!(builder.url_for(&reference("pub-1")), None);
        // And a base that is only whitespace is no base.
        assert_eq!(AppPublicationUrl::new("   ").url_for(&reference("x")), None);
    }

    #[test]
    fn the_configured_origin_is_taken_from_the_instance_configuration() {
        let config = AppConfig {
            public_base_url: Some("https://gw.example".into()),
            ..AppConfig::default()
        };
        assert_eq!(
            AppPublicationUrl::from_config(&config)
                .url_for(&reference("abc"))
                .as_deref(),
            Some("https://gw.example/publications/abc")
        );
    }
}
