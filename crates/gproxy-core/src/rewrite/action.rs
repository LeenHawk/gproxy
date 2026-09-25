use gproxy_protocol::Dialect;
use serde::Deserialize;
use serde_json::Value;

use super::RewriteCompileError;
use crate::{RewritePhase, RewriteTarget};

#[derive(Debug)]
pub enum RuleAction {
    Replace,
    Set(Value),
    Delete,
    Merge(Value),
    HeaderSet(http::HeaderValue),
    HeaderMerge(http::HeaderValue),
    SystemText(SystemTextConfig),
    CacheBreakpoint(CacheBreakpointConfig),
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPosition {
    Prepend,
    Append,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemTextConfig {
    pub dialect: String,
    pub text: String,
    pub position: TextPosition,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheBreakpointConfig {
    pub dialect: String,
    pub target: String,
    pub index: Option<i64>,
    pub ttl: Option<String>,
}

impl RuleAction {
    pub fn dialect(&self) -> Option<Dialect> {
        match self {
            Self::SystemText(c) => Dialect::from_id(&c.dialect),
            Self::CacheBreakpoint(c) => Dialect::from_id(&c.dialect),
            _ => None,
        }
    }
}

pub(super) fn compile(
    action: &str,
    replacement: &str,
    target: &RewriteTarget,
    phase: RewritePhase,
    event: bool,
) -> Result<RuleAction, RewriteCompileError> {
    let invalid = |message: &str| RewriteCompileError::InvalidAction(message.into());
    let json = || serde_json::from_str::<Value>(replacement).map_err(|e| invalid(&e.to_string()));
    match action {
        "replace" => Ok(RuleAction::Replace),
        "set" | "delete" | "merge" => {
            if !matches!(target, RewriteTarget::Body { paths: Some(_) }) {
                return Err(invalid("JSON actions require body paths"));
            }
            match action {
                "set" => Ok(RuleAction::Set(json()?)),
                "delete" => Ok(RuleAction::Delete),
                _ => {
                    let value = json()?;
                    if !value.is_object() {
                        return Err(invalid("merge requires a JSON object"));
                    }
                    Ok(RuleAction::Merge(value))
                }
            }
        }
        "header_set" | "header_merge" => {
            if !matches!(target, RewriteTarget::Header { .. }) {
                return Err(invalid("header actions require a header target"));
            }
            let value =
                http::HeaderValue::from_str(replacement).map_err(|e| invalid(&e.to_string()))?;
            Ok(if action == "header_set" {
                RuleAction::HeaderSet(value)
            } else {
                RuleAction::HeaderMerge(value)
            })
        }
        "system_text" | "cache_breakpoint" => {
            if !matches!(target, RewriteTarget::Body { paths: None })
                || phase != RewritePhase::Request
                || event
            {
                return Err(invalid(
                    "content actions require an unfiltered request body without paths or event filters",
                ));
            }
            if action == "system_text" {
                let config: SystemTextConfig =
                    serde_json::from_str(replacement).map_err(|e| invalid(&e.to_string()))?;
                if !matches!(
                    config.dialect.as_str(),
                    "claude" | "openai_chat" | "openai" | "openai_responses_websocket" | "gemini"
                ) || config.text.is_empty()
                {
                    return Err(invalid(
                        "system text requires a supported dialect and nonempty text",
                    ));
                }
                Ok(RuleAction::SystemText(config))
            } else {
                let config: CacheBreakpointConfig =
                    serde_json::from_str(replacement).map_err(|e| invalid(&e.to_string()))?;
                if !matches!(
                    config.dialect.as_str(),
                    "claude" | "openai_chat" | "openai" | "openai_responses_websocket"
                ) || !matches!(
                    config.target.as_str(),
                    "global" | "system" | "message" | "tools"
                ) || config.index == Some(0)
                {
                    return Err(invalid("invalid cache dialect, target or index"));
                }
                if config.dialect != "claude" && config.target == "tools" {
                    return Err(invalid("tool breakpoints require Claude"));
                }
                if let Some(ttl) = &config.ttl {
                    let valid = if config.dialect == "claude" {
                        matches!(ttl.as_str(), "5m" | "1h")
                    } else {
                        ttl == "30m"
                    };
                    if !valid {
                        return Err(invalid("unsupported cache TTL for dialect"));
                    }
                }
                Ok(RuleAction::CacheBreakpoint(config))
            }
        }
        _ => Err(invalid(action)),
    }
}
