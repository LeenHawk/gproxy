//! Credential-scoped control-plane operations.
//!
//! These are deliberately separate from model-routed data-plane operations:
//! callers must name one concrete stored credential, so account metadata never
//! leaks through the public aggregate gateway.

use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::Value;

use crate::usage::{RateLimitResetCreditConsumeResponse, UsageSnapshot};

#[derive(Debug, Clone)]
pub enum CredentialControlOperation {
    Usage,
    ListRateLimitResetCredits,
    ConsumeRateLimitResetCredit { idempotency_key: String },
    Account,
    Profile,
    Settings,
    ListTasks { query: Option<String> },
    GetTask { task_id: String },
    ListSiblingTurns { task_id: String, turn_id: String },
    CreateTask { body: Value },
    /// Explicitly allowlisted Codex backend operation used by the public PAT
    /// service surface. The Codex bulletin rehomes it onto the selected
    /// subscription account and injects that credential's auth headers.
    CodexRaw {
        label: &'static str,
        method: Method,
        path: String,
        query: Option<String>,
        headers: HeaderMap,
        body: Bytes,
    },
}

impl CredentialControlOperation {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::ListRateLimitResetCredits => "rate_limit_reset_credits",
            Self::ConsumeRateLimitResetCredit { .. } => "rate_limit_reset_credit_consume",
            Self::Account => "account",
            Self::Profile => "profile",
            Self::Settings => "settings",
            Self::ListTasks { .. } => "tasks_list",
            Self::GetTask { .. } => "task_get",
            Self::ListSiblingTurns { .. } => "task_sibling_turns",
            Self::CreateTask { .. } => "task_create",
            Self::CodexRaw { label, .. } => label,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CredentialControlResponse {
    Usage(UsageSnapshot),
    RateLimitResetCreditConsume(RateLimitResetCreditConsumeResponse),
    Json(Value),
}
