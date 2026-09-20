//! URL publication through a host link builder. Core has no public HTTP
//! surface, so it cannot mint a link itself; it owns the bytes and the id and
//! lets the host build the URL and serve the bytes on its own route through
//! `Core::read_publication`.

use gproxy_protocol::{HttpBody, capability::ResourceMetadata};

/// Builds the public URL for a publication core stores. The host owns the
/// route that serves it and any signing or expiry it applies to the link.
/// Pure and cheap: it is called inside the adapt flow, before the body is
/// stored, so a refusal has no side effect.
pub trait PublicationUrl: Send + Sync {
    /// `None` means the host cannot expose this publication; the publish then
    /// fails with `Unsupported` before anything is written.
    fn url_for(&self, publication: &PublicationRef<'_>) -> Option<String>;
}

/// What the host gets to build a link from. `id` is the binding id core is
/// about to record (`resource_bindings.id`), the same value
/// `Core::read_publication` accepts back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationRef<'a> {
    pub id: &'a str,
    /// The caller scope column the publication belongs to.
    pub scope: &'a str,
    pub mime: Option<&'a str>,
    pub expires_at_ms: Option<i64>,
}

/// A stored publication read back for the host's download route, from
/// `Core::read_publication`.
pub struct Publication {
    pub id: String,
    pub scope: String,
    pub metadata: ResourceMetadata,
    pub body: HttpBody,
}
