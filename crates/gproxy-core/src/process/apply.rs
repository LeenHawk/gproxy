use bytes::Bytes;
use http::HeaderMap;

use super::{CompiledRule, RuleConfig, TransformLocate, content, generic};

#[derive(Debug, Clone, Copy)]
pub struct RuleModels<'a> {
    primary: &'a str,
    alternate: Option<&'a str>,
}

impl<'a> RuleModels<'a> {
    pub const fn new(primary: &'a str, alternate: Option<&'a str>) -> Self {
        Self { primary, alternate }
    }

    pub(crate) fn owned(self) -> (String, Option<String>) {
        (self.primary.into(), self.alternate.map(str::to_owned))
    }
}

pub struct RequestMutation {
    pub body: Bytes,
    pub headers: Option<HeaderMap>,
}

pub fn apply_request(
    rules: &[CompiledRule],
    operation: gproxy_protocol::OperationKey,
    models: RuleModels<'_>,
    client_headers: &HeaderMap,
    body: Bytes,
) -> RequestMutation {
    let applicable = applicable(rules, operation, models, client_headers);
    if applicable.is_empty() {
        return RequestMutation {
            body,
            headers: None,
        };
    }
    let kind = match operation.kind {
        gproxy_protocol::OperationKind::ContentGeneration(kind) => Some(kind),
        gproxy_protocol::OperationKind::Family(_) => None,
    };
    let mut value = None;
    let mut value_changed = false;
    for rule in applicable.iter().filter(|rule| request_value(&rule.config)) {
        if value.is_none() {
            let Ok(parsed) = serde_json::from_slice(&body) else {
                break;
            };
            value = Some(parsed);
        }
        let current = value.as_mut().expect("parsed value");
        value_changed |= match &rule.config {
            RuleConfig::SystemText { text, position } => {
                content::system_text(current, kind, text, *position)
            }
            RuleConfig::CacheBreakpoint(config) => content::cache_breakpoint(current, kind, config),
            RuleConfig::Rewrite {
                path,
                action,
                value,
            } => generic::rewrite(current, path, *action, value.as_ref()),
            RuleConfig::Transform(config) => generic::transform_value(current, config),
            RuleConfig::Header { .. } => false,
        };
    }
    let mut body = if value_changed {
        serde_json::to_vec(value.as_ref().expect("changed value"))
            .map(Bytes::from)
            .unwrap_or(body)
    } else {
        body
    };
    for rule in applicable.iter().filter(|rule| request_text(&rule.config)) {
        let RuleConfig::Transform(config) = &rule.config else {
            continue;
        };
        body = generic::transform_text(body, config);
    }
    let header_rules = applicable
        .iter()
        .filter_map(|rule| match &rule.config {
            RuleConfig::Header { name, value, mode } => Some((name, value, mode)),
            RuleConfig::SystemText { .. }
            | RuleConfig::CacheBreakpoint(_)
            | RuleConfig::Rewrite { .. }
            | RuleConfig::Transform(_) => None,
        })
        .collect::<Vec<_>>();
    let headers = (!header_rules.is_empty()).then(|| {
        let mut headers = client_headers.clone();
        for (name, value, mode) in header_rules {
            generic::header(&mut headers, name, value, *mode);
        }
        headers
    });
    RequestMutation { body, headers }
}

pub fn apply_response(
    rules: &[CompiledRule],
    operation: gproxy_protocol::OperationKey,
    models: RuleModels<'_>,
    client_headers: &HeaderMap,
    body: Bytes,
) -> Bytes {
    let applicable = applicable(rules, operation, models, client_headers);
    let mut value = None;
    let mut changed = false;
    for rule in applicable
        .iter()
        .filter(|rule| response_value(&rule.config))
    {
        if value.is_none() {
            let Ok(parsed) = serde_json::from_slice(&body) else {
                break;
            };
            value = Some(parsed);
        }
        let RuleConfig::Transform(config) = &rule.config else {
            continue;
        };
        changed |= generic::transform_value(value.as_mut().expect("parsed value"), config);
    }
    let mut body = if changed {
        serde_json::to_vec(value.as_ref().expect("changed value"))
            .map(Bytes::from)
            .unwrap_or(body)
    } else {
        body
    };
    for rule in applicable.iter().filter(|rule| response_text(&rule.config)) {
        let RuleConfig::Transform(config) = &rule.config else {
            continue;
        };
        body = generic::transform_text(body, config);
    }
    body
}

fn applicable<'a>(
    rules: &'a [CompiledRule],
    operation: gproxy_protocol::OperationKey,
    models: RuleModels<'_>,
    headers: &HeaderMap,
) -> Vec<&'a CompiledRule> {
    rules
        .iter()
        .filter(|rule| {
            rule.operations
                .as_ref()
                .is_none_or(|operations| operations.contains(&operation.operation))
                && rule.model_pattern.as_ref().is_none_or(|pattern| {
                    pattern.is_match(models.primary)
                        || models
                            .alternate
                            .is_some_and(|alternate| pattern.is_match(alternate))
                })
                && rule
                    .header_pattern
                    .as_ref()
                    .is_none_or(|pattern| header_matches(pattern, headers))
        })
        .collect()
}

fn header_matches(pattern: &regex::Regex, headers: &HeaderMap) -> bool {
    headers.iter().any(|(name, value)| {
        value
            .to_str()
            .ok()
            .is_some_and(|value| pattern.is_match(&format!("{}: {}", name.as_str(), value.trim())))
    })
}

fn request_value(config: &RuleConfig) -> bool {
    match config {
        RuleConfig::SystemText { .. }
        | RuleConfig::CacheBreakpoint(_)
        | RuleConfig::Rewrite { .. } => true,
        RuleConfig::Transform(config) => {
            config.phase.request() && !matches!(config.locate, TransformLocate::Match(_))
        }
        RuleConfig::Header { .. } => false,
    }
}

fn request_text(config: &RuleConfig) -> bool {
    matches!(config, RuleConfig::Transform(config) if config.phase.request() && matches!(config.locate, TransformLocate::Match(_)))
}

fn response_value(config: &RuleConfig) -> bool {
    matches!(config, RuleConfig::Transform(config) if config.phase.response() && !matches!(config.locate, TransformLocate::Match(_)))
}

fn response_text(config: &RuleConfig) -> bool {
    matches!(config, RuleConfig::Transform(config) if config.phase.response() && matches!(config.locate, TransformLocate::Match(_)))
}
