//! Azure's hosted first-party models: Azure OpenAI, and the Anthropic models
//! Azure AI Foundry serves beside them.
//!
//! The wire is the vendor's own. An OpenAI Responses or Chat body and a Claude
//! Messages body both go upstream byte for byte; what Azure changes is *where*
//! the method lives and *how* the key is presented, and that is the whole job
//! of this channel.
//!
//! Wire facts, from Azure's REST reference ("Azure OpenAI in Azure AI Foundry
//! Models REST API", "Anthropic Claude in Foundry Models") and from the v3
//! channel this replaces (`crates/gproxy-channels/src/azure/` on `main`):
//!
//! - The origin is `https://{resource}.openai.azure.com`, unless the provider
//!   row carries its own `base_url` (a Foundry project, a private endpoint or
//!   an APIM front door).
//! - The **v1 surface** mounts each vendor's own path under a family prefix:
//!   `/openai` + `/v1/responses`, `/v1/chat/completions`, `/v1/models`,
//!   `/v1/embeddings`, `/v1/images/generations`, `/v1/videos/…`; and
//!   `/anthropic` + `/v1/messages`, `/v1/messages/count_tokens`. This surface
//!   needs no `api-version`.
//! - The older **deployment surface**, selected by configuring `deployment`,
//!   is `/openai/deployments/{deployment}` plus the vendor path with its `/v1`
//!   prefix removed, and it does require `api-version`.
//! - The key rides in `api-key` for the OpenAI family, and in `x-api-key` with
//!   `anthropic-version` for Claude. Azure never uses `Authorization: Bearer`
//!   for key authentication.
//! - Gemini is not served here at all.
//!
//! Nothing in this channel impersonates a vendor client, so there is no
//! `ChannelHeaders` set and no `default_connection`: Azure fingerprints
//! nothing, and an operator's own connection profile is the only thing that
//! should decide the outbound stack.

mod config;
mod endpoint;
mod request;

pub use config::AzureConfig;

pub const ID: &str = "azure";

/// Stateless: one instance serves every Azure provider.
#[derive(Debug, Default, Clone, Copy)]
pub struct Azure;
