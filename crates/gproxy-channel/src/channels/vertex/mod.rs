//! Google Vertex AI: Google's own models plus the partner publishers Vertex
//! resells, addressed through a Google Cloud project and a region.
//!
//! The wire is each publisher's own — a Gemini `GenerateContent` body and a
//! Claude Messages body both go upstream essentially unchanged. What Vertex
//! changes is the resource hierarchy the method lives in and how the caller
//! proves who it is, and that is what this channel exists for.
//!
//! Wire facts, from Google's Vertex AI REST reference (`generateContent`,
//! `countTokens`, `embedContent`, the `publishers/anthropic` `rawPredict`
//! surface and the OpenAI-compatible `endpoints/openapi` surface) and from the
//! v3 channel this replaces (`crates/gproxy-channels/src/vertex/` on `main`):
//!
//! - The origin is regional: `https://{location}-aiplatform.googleapis.com`,
//!   or `https://aiplatform.googleapis.com` when the location is `global`.
//! - Google's own models live at `/v1beta1/projects/{project}/locations/
//!   {location}/publishers/google/models/{model}:{verb}`. The model directory
//!   is project-free: `/v1beta1/publishers/google/models`.
//! - Anthropic's models live at `/v1/projects/{project}/locations/{location}/
//!   publishers/anthropic/models/{model}:rawPredict`, `:streamRawPredict` for
//!   a stream, and the token counter is the literal model `count-tokens`.
//!   Their body carries `anthropic_version: "vertex-2023-10-16"` and must not
//!   carry `model`, which the URL already names.
//! - The OpenAI-compatible Chat surface is
//!   `/v1beta1/projects/{project}/locations/{location}/endpoints/openapi/chat/completions`.
//! - Authentication is `Authorization: Bearer {access_token}`, where the token
//!   is minted from a service-account key. See [`auth`]: the mint is a
//!   `CredentialRefresh`, never something `prepare` does.
//!
//! Vertex Express is a separate channel (`vertexexpress`): it shares this
//! upstream's Gemini body but not its resource hierarchy, its operation
//! surface or its credential. See `design/channels.md`.
//!
//! Nothing here impersonates a vendor client, so there is no `ChannelHeaders`
//! set and no `default_connection`.

mod auth;
mod config;
mod endpoint;
mod request;

pub use config::VertexConfig;

pub const ID: &str = "vertex";

/// Stateless: one instance serves every Vertex provider.
#[derive(Debug, Default, Clone, Copy)]
pub struct Vertex;
