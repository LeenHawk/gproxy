//! One persisted RewriteRule row into its executable form, enforcing the
//! target/phase/name rules from `design/core-rewrite.md`.

use crate::{PathSegment, RewritePhase, RewriteRuleData, RewriteTarget};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_store::entity::upstream::rewrite_rule::{self, RewriteTarget as StoredTarget};
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashSet, sync::Arc};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RewriteCompileError {
    #[error("unknown phase `{0}`")]
    InvalidPhase(String),
    #[error("header and query targets require target_name")]
    MissingTargetName,
    #[error("target_name is only valid for header and query targets")]
    NameOnBody,
    #[error("`{0}` is not a valid header name")]
    InvalidHeaderName(String),
    #[error("paths are only valid for the body target")]
    PathsOnNonBody,
    #[error("event filters are only valid for the body target")]
    EventFilterOnNonBody,
    #[error("query rules may only run in the request phase")]
    QueryResponsePhase,
    #[error("invalid paths: {0}")]
    InvalidPaths(String),
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("invalid filter_operation_keys: {0}")]
    InvalidOperationKeys(String),
    #[error("invalid filter_model_pattern")]
    InvalidModelPattern,
    #[error("invalid filter_header_pattern: {0}")]
    InvalidHeaderPattern(String),
    #[error("invalid filter_event_pattern: {0}")]
    InvalidEventPattern(String),
}

/// Compile one enabled rule. Disabled rows are the caller's concern.
pub fn compile_rule(
    entity: Arc<rewrite_rule::Model>,
) -> Result<RewriteRuleData, RewriteCompileError> {
    let phase = match entity.phase.as_str() {
        "request" => RewritePhase::Request,
        "response" => RewritePhase::Response,
        "both" => RewritePhase::Both,
        other => return Err(RewriteCompileError::InvalidPhase(other.to_owned())),
    };
    let name = entity
        .target_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let target = match entity.target {
        StoredTarget::Body => {
            if name.is_some() {
                return Err(RewriteCompileError::NameOnBody);
            }
            RewriteTarget::Body {
                paths: entity.paths.as_ref().map(parse_paths).transpose()?,
            }
        }
        StoredTarget::Header | StoredTarget::Query => {
            let name = name.ok_or(RewriteCompileError::MissingTargetName)?;
            if entity.paths.is_some() {
                return Err(RewriteCompileError::PathsOnNonBody);
            }
            if entity.filter_event_pattern.is_some() {
                return Err(RewriteCompileError::EventFilterOnNonBody);
            }
            if entity.target == StoredTarget::Header {
                RewriteTarget::Header {
                    name: name
                        .parse()
                        .map_err(|_| RewriteCompileError::InvalidHeaderName(name.to_owned()))?,
                }
            } else {
                if phase != RewritePhase::Request {
                    return Err(RewriteCompileError::QueryResponsePhase);
                }
                RewriteTarget::Query {
                    name: name.to_owned(),
                }
            }
        }
    };
    let pattern = Regex::new(&entity.pattern)
        .map_err(|error| RewriteCompileError::InvalidPattern(error.to_string()))?;
    let operation_keys = entity
        .filter_operation_keys
        .as_ref()
        .map(parse_operation_keys)
        .transpose()?;
    let model_matcher = entity
        .filter_model_pattern
        .as_deref()
        .map(glob_to_regex)
        .transpose()?;
    let header_matcher = entity
        .filter_header_pattern
        .as_deref()
        .map(|pattern| {
            Regex::new(&format!("(?i){pattern}"))
                .map_err(|error| RewriteCompileError::InvalidHeaderPattern(error.to_string()))
        })
        .transpose()?;
    let event_matcher = entity
        .filter_event_pattern
        .as_deref()
        .map(|pattern| {
            Regex::new(pattern)
                .map_err(|error| RewriteCompileError::InvalidEventPattern(error.to_string()))
        })
        .transpose()?;
    Ok(RewriteRuleData {
        entity,
        phase,
        pattern,
        target,
        operation_keys,
        model_matcher,
        header_matcher,
        event_matcher,
    })
}

/// `tools.*.name` → keys, numeric indexes and wildcards. Not JSONPath.
fn parse_paths(value: &serde_json::Value) -> Result<Vec<Vec<PathSegment>>, RewriteCompileError> {
    let paths: Vec<String> = serde_json::from_value(value.clone())
        .map_err(|error| RewriteCompileError::InvalidPaths(error.to_string()))?;
    if paths.is_empty() {
        return Err(RewriteCompileError::InvalidPaths("empty array".into()));
    }
    paths
        .iter()
        .map(|path| {
            let segments = path
                .split('.')
                .map(|segment| match segment {
                    "" => Err(RewriteCompileError::InvalidPaths(format!(
                        "empty segment in `{path}`"
                    ))),
                    "*" => Ok(PathSegment::Wildcard),
                    digits if digits.bytes().all(|b| b.is_ascii_digit()) => {
                        digits.parse().map(PathSegment::Index).map_err(|_| {
                            RewriteCompileError::InvalidPaths(format!("index overflow in `{path}`"))
                        })
                    }
                    key => Ok(PathSegment::Key(key.to_owned())),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(segments)
        })
        .collect()
}

fn parse_operation_keys(
    value: &serde_json::Value,
) -> Result<HashSet<OperationKey>, RewriteCompileError> {
    #[derive(Deserialize)]
    struct Pair {
        operation: String,
        dialect: String,
    }
    let pairs: Vec<Pair> = serde_json::from_value(value.clone())
        .map_err(|error| RewriteCompileError::InvalidOperationKeys(error.to_string()))?;
    pairs
        .iter()
        .map(|pair| {
            Ok(OperationKey {
                operation: Operation::from_id(&pair.operation).ok_or_else(|| {
                    RewriteCompileError::InvalidOperationKeys(format!(
                        "unknown operation `{}`",
                        pair.operation
                    ))
                })?,
                dialect: Dialect::from_id(&pair.dialect).ok_or_else(|| {
                    RewriteCompileError::InvalidOperationKeys(format!(
                        "unknown dialect `{}`",
                        pair.dialect
                    ))
                })?,
            })
        })
        .collect()
}

/// `*` and `?` globs over the whole model name, case-sensitive and anchored.
/// Shared by rewrite filters, price rules and budget model patterns.
pub(crate) fn glob_to_regex(glob: &str) -> Result<Regex, RewriteCompileError> {
    let mut pattern = String::with_capacity(glob.len() + 4);
    pattern.push('^');
    for ch in glob.chars() {
        match ch {
            '*' => pattern.push_str(".*"),
            '?' => pattern.push('.'),
            other => pattern.push_str(&regex::escape(&other.to_string())),
        }
    }
    pattern.push('$');
    Regex::new(&pattern).map_err(|_| RewriteCompileError::InvalidModelPattern)
}
