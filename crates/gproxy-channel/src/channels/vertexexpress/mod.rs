//! Google Vertex AI Express mode: the same Gemini models as `vertex`, reached
//! with an API key instead of a Google Cloud identity.
//!
//! Express mode is a separate channel rather than an authentication mode of
//! [`super::vertex`] on purpose. The two share a Gemini request body and a
//! host name, and nothing else: express mode has no project and no region, so
//! its methods sit outside the `projects/{p}/locations/{l}` hierarchy
//! entirely; it serves only Google's own publisher, so there is no Anthropic
//! or OpenAI-compatible surface; and its provider form has no keys at all
//! where `vertex` has two required ones. See `design/channels.md`.
//!
//! Wire facts, from Google's Vertex AI Express mode reference and from the v3
//! channel this replaces (`crates/gproxy-channels/src/vertexexpress/` on
//! `main`):
//!
//! - One global origin, `https://aiplatform.googleapis.com`; there is no
//!   regional host because an express key is not bound to a region.
//! - Methods are `/v1/publishers/google/models/{model}:{verb}` for
//!   `generateContent`, `streamGenerateContent` and `countTokens`.
//! - The key travels as the `key` query parameter. It is never put in a
//!   header here, and an `endpoint_override` that already embeds a `key` is
//!   refused rather than silently sending two.
//!
//! Nothing here impersonates a vendor client, so there is no `ChannelHeaders`
//! set and no `default_connection`.

mod endpoint;
mod request;

pub const ID: &str = "vertexexpress";

/// Stateless: one instance serves every Vertex Express provider.
#[derive(Debug, Default, Clone, Copy)]
pub struct VertexExpress;
