//! Apply selected rules to one head or one payload unit. Nothing here reads
//! configuration or decodes a transport; callers hand in exactly one value.

use super::{RewriteError, RuleAction, json_path::rewrite_at_paths};
use crate::{RewriteRuleData, RewriteTarget};
use http::{HeaderMap, HeaderValue};
use std::{borrow::Cow, sync::Arc};

/// Replace within every value of each rule's header. Repeated values keep
/// their count and order. Returns whether anything changed.
pub fn apply_headers(
    rules: &[Arc<RewriteRuleData>],
    headers: &mut HeaderMap,
) -> Result<bool, RewriteError> {
    let mut changed_any = false;
    for rule in rules {
        let RewriteTarget::Header { name } = &rule.target else {
            continue;
        };
        match &rule.action {
            RuleAction::HeaderSet(value) => {
                changed_any |=
                    headers.get_all(name).iter().count() != 1 || headers.get(name) != Some(value);
                headers.insert(name.clone(), value.clone());
                continue;
            }
            RuleAction::HeaderMerge(value) => {
                let existing = headers
                    .get_all(name)
                    .iter()
                    .map(|value| {
                        value
                            .to_str()
                            .map_err(|_| RewriteError::NonTextHeader(name.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let incoming = value
                    .to_str()
                    .map_err(|_| RewriteError::NonTextHeader(name.to_string()))?;
                if !existing
                    .iter()
                    .flat_map(|value| value.split(','))
                    .any(|token| token.trim() == incoming.trim())
                {
                    let merged = if existing.is_empty() {
                        incoming.to_owned()
                    } else {
                        format!("{},{incoming}", existing.join(","))
                    };
                    headers.insert(
                        name.clone(),
                        HeaderValue::from_str(&merged)
                            .map_err(|_| RewriteError::InvalidHeaderValue(name.to_string()))?,
                    );
                    changed_any = true;
                }
                continue;
            }
            _ => {}
        }
        let mut values = Vec::new();
        let mut changed = false;
        for value in headers.get_all(name) {
            let text = value
                .to_str()
                .map_err(|_| RewriteError::NonTextHeader(name.to_string()))?;
            match rule
                .pattern
                .replace_all(text, rule.entity.replacement.as_str())
            {
                Cow::Borrowed(_) => values.push(value.clone()),
                Cow::Owned(replaced) => {
                    changed = true;
                    values.push(
                        HeaderValue::from_str(&replaced)
                            .map_err(|_| RewriteError::InvalidHeaderValue(name.to_string()))?,
                    );
                }
            }
        }
        if changed {
            headers.remove(name);
            for value in values {
                headers.append(name.clone(), value);
            }
            changed_any = true;
        }
    }
    Ok(changed_any)
}

/// Replace within every value of each rule's parameter. Order, repeats and
/// untouched raw segments are preserved byte for byte; only rewritten values
/// are encoded again. Returns the new query when anything changed.
pub fn apply_query(
    rules: &[Arc<RewriteRuleData>],
    query: Option<&str>,
) -> Result<Option<String>, RewriteError> {
    let Some(query) = query else {
        return Ok(None);
    };
    let mut segments: Vec<Cow<'_, str>> = query.split('&').map(Cow::Borrowed).collect();
    let mut changed = false;
    for rule in rules {
        let RewriteTarget::Query { name } = &rule.target else {
            continue;
        };
        for segment in &mut segments {
            let Some((raw_name, raw_value)) = segment.split_once('=') else {
                continue;
            };
            if form_decode(raw_name) != *name {
                continue;
            }
            let value = form_decode(raw_value);
            if let Cow::Owned(replaced) = rule
                .pattern
                .replace_all(&value, rule.entity.replacement.as_str())
            {
                let encoded: String =
                    form_urlencoded::byte_serialize(replaced.as_bytes()).collect();
                *segment = Cow::Owned(format!("{raw_name}={encoded}"));
                changed = true;
            }
        }
    }
    Ok(changed.then(|| segments.join("&")))
}

fn form_decode(raw: &str) -> String {
    let plus_to_space = raw.replace('+', " ");
    percent_encoding::percent_decode_str(&plus_to_space)
        .decode_utf8_lossy()
        .into_owned()
}

/// Rewrite one complete body (buffered HTTP body). Event filters do not apply
/// to whole bodies. Returns the new bytes only when something changed, so an
/// untouched body keeps its original representation.
pub fn apply_body(
    rules: &[Arc<RewriteRuleData>],
    body: &[u8],
) -> Result<Option<Vec<u8>>, RewriteError> {
    if rules.is_empty() {
        return Ok(None);
    }
    let text = std::str::from_utf8(body).map_err(|_| RewriteError::NonUtf8Body)?;
    Ok(apply_text(rules.iter().map(Arc::as_ref), text)?.map(String::into_bytes))
}

/// Rewrite one decoded stream/WS unit. `event_type` is the SSE event name
/// (falling back to the JSON `type`) or the WS JSON `type`; units without a
/// type only match rules without an event filter.
pub fn apply_unit(
    rules: &[Arc<RewriteRuleData>],
    event_type: Option<&str>,
    data: &str,
) -> Result<Option<String>, RewriteError> {
    let applicable = rules
        .iter()
        .filter(|rule| {
            rule.action.dialect() != Some(gproxy_protocol::Dialect::OpenAiResponsesWebSocket)
                || event_type == Some("response.create")
        })
        .filter(|rule| match &rule.event_matcher {
            None => true,
            Some(matcher) => event_type.is_some_and(|event| matcher.is_match(event)),
        })
        .map(Arc::as_ref);
    apply_text(applicable, data)
}

/// Rules interleave in order, each on the previous rule's output. Regex path
/// replacements splice selected strings and preserve all other bytes. Set
/// actions parse JSON and assign typed values, creating missing object keys;
/// malformed JSON is an error rather than an apparently successful assignment.
fn apply_text<'a>(
    rules: impl Iterator<Item = &'a RewriteRuleData>,
    input: &str,
) -> Result<Option<String>, RewriteError> {
    let mut payload = Payload {
        text: Cow::Borrowed(input),
        document: None,
        jmespath: None,
        dirty: false,
        changed: false,
    };
    for rule in rules {
        let RewriteTarget::Body { paths } = &rule.target else {
            continue;
        };
        if let Some(condition) = &rule.body_condition {
            // Invalid/non-JSON units cannot satisfy a JSON condition, including absence.
            if !payload.matches(condition) {
                continue;
            }
        }
        if !matches!(rule.action, RuleAction::Replace) {
            let document = payload.json()?;
            let changed = match &rule.action {
                RuleAction::Set(_) | RuleAction::Delete | RuleAction::Merge(_) => {
                    let mut changed = false;
                    for path in paths.as_ref().expect("compiled JSON paths") {
                        changed |= super::json_edit::apply(document, path, &rule.action)?;
                    }
                    changed
                }
                // Native content helpers can canonicalize while returning false, or
                // return true for an existing marker. Keep their established no-op semantics.
                RuleAction::SystemText(_) | RuleAction::CacheBreakpoint(_) => {
                    let before = document.clone();
                    let applied = match &rule.action {
                        RuleAction::SystemText(config) => super::content::system_text(
                            document,
                            rule.action.dialect(),
                            &config.text,
                            config.position,
                        ),
                        RuleAction::CacheBreakpoint(config) => {
                            super::cache::apply(document, rule.action.dialect(), config)
                        }
                        _ => unreachable!(),
                    };
                    if !applied {
                        *document = before;
                        false
                    } else {
                        *document != before
                    }
                }
                _ => unreachable!("compiled body action"),
            };
            if changed {
                payload.jmespath = None;
            }
            payload.dirty |= changed;
            payload.changed |= changed;
            continue;
        }
        payload.flush();
        let next = match paths {
            None => match rule
                .pattern
                .replace_all(&payload.text, rule.entity.replacement.as_str())
            {
                Cow::Borrowed(_) => None,
                Cow::Owned(replaced) => Some(replaced),
            },
            Some(paths) => rewrite_at_paths(&payload.text, paths, &mut |text| match rule
                .pattern
                .replace_all(text, rule.entity.replacement.as_str())
            {
                Cow::Borrowed(_) => None,
                Cow::Owned(replaced) => Some(replaced),
            }),
        };
        if let Some(next) = next {
            payload.text = Cow::Owned(next);
            payload.document = None;
            payload.jmespath = None;
            payload.changed = true;
        }
    }
    payload.flush();
    Ok(payload.changed.then(|| payload.text.into_owned()))
}

/// A body/unit owns one lazy parsed representation until a text edit invalidates it.
/// Failed parses are cached too, so several nonmatching predicates scan only once.
struct Payload<'a> {
    text: Cow<'a, str>,
    document: Option<Result<serde_json::Value, String>>,
    jmespath: Option<Result<jmespath::Rcvar, jmespath::JmespathError>>,
    dirty: bool,
    changed: bool,
}
impl Payload<'_> {
    fn matches(&mut self, condition: &super::BodyCondition) -> bool {
        if self.json().is_err() {
            return false;
        }
        let document = self.document.as_ref().unwrap().as_ref().unwrap();
        condition.matches(document, &mut self.jmespath)
    }
    fn json(&mut self) -> Result<&mut serde_json::Value, RewriteError> {
        self.document
            .get_or_insert_with(|| serde_json::from_str(&self.text).map_err(|e| e.to_string()))
            .as_mut()
            .map_err(|error| RewriteError::InvalidJson(error.clone()))
    }
    fn flush(&mut self) {
        if self.dirty {
            self.text = Cow::Owned(
                self.document
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .to_string(),
            );
            self.dirty = false;
        }
    }
}
