//! Credential-scoped control-plane operations.
//!
//! These are deliberately separate from model-routed data-plane operations:
//! callers must name one concrete stored credential, so account metadata never
//! leaks through the public aggregate gateway.

use serde_json::Value;

use crate::usage::{RateLimitResetCreditConsumeResponse, UsageSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
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
        }
    }
}

#[derive(Debug, Clone)]
pub enum CredentialControlResponse {
    Usage(UsageSnapshot),
    RateLimitResetCreditConsume(RateLimitResetCreditConsumeResponse),
    Json(Value),
}
