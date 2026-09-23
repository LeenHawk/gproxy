//! Apply selected rules to one head or one payload unit. Nothing here reads
//! configuration or decodes a transport; callers hand in exactly one value.

use super::{RewriteError, json_path::rewrite_at_paths};
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
    Ok(apply_text(rules, text)?.map(String::into_bytes))
}

/// Rewrite one decoded stream/WS unit. `event_type` is the SSE event name
/// (falling back to the JSON `type`) or the WS JSON `type`; units without a
/// type only match rules without an event filter.
pub fn apply_unit(
    rules: &[Arc<RewriteRuleData>],
    event_type: Option<&str>,
    data: &str,
) -> Result<Option<String>, RewriteError> {
    let applicable: Vec<Arc<RewriteRuleData>> = rules
        .iter()
        .filter(|rule| match &rule.event_matcher {
            None => true,
            Some(matcher) => event_type.is_some_and(|event| matcher.is_match(event)),
        })
        .cloned()
        .collect();
    if applicable.is_empty() {
        return Ok(None);
    }
    apply_text(&applicable, data)
}

/// Rules interleave in order, each on the previous rule's output. Regex path
/// replacements splice selected strings and preserve all other bytes. Set
/// actions parse JSON and assign typed values, creating missing object keys;
/// malformed JSON is an error rather than an apparently successful assignment.
fn apply_text(rules: &[Arc<RewriteRuleData>], input: &str) -> Result<Option<String>, RewriteError> {
    let mut current: Cow<'_, str> = Cow::Borrowed(input);
    let mut changed = false;
    for rule in rules {
        let RewriteTarget::Body { paths } = &rule.target else {
            continue;
        };
        let next = if let Some(value) = &rule.set_value {
            let mut document: serde_json::Value = serde_json::from_str(&current)
                .map_err(|error| RewriteError::InvalidJson(error.to_string()))?;
            let before = document.clone();
            for path in paths.as_ref().expect("compiled set paths") {
                set_json(&mut document, path, value)?;
            }
            if document == before {
                None
            } else {
                Some(document.to_string())
            }
        } else {
            match paths {
                None => match rule
                    .pattern
                    .replace_all(&current, rule.entity.replacement.as_str())
                {
                    Cow::Borrowed(_) => None,
                    Cow::Owned(replaced) => Some(replaced),
                },
                Some(paths) => rewrite_at_paths(&current, paths, &mut |text| match rule
                    .pattern
                    .replace_all(text, rule.entity.replacement.as_str())
                {
                    Cow::Borrowed(_) => None,
                    Cow::Owned(replaced) => Some(replaced),
                }),
            }
        };
        if let Some(next) = next {
            current = Cow::Owned(next);
            changed = true;
        }
    }
    Ok(changed.then(|| current.into_owned()))
}

fn set_json(
    current: &mut serde_json::Value,
    path: &[crate::PathSegment],
    value: &serde_json::Value,
) -> Result<(), RewriteError> {
    use crate::PathSegment;
    use serde_json::{Value, json};
    let Some((segment, rest)) = path.split_first() else {
        *current = value.clone();
        return Ok(());
    };
    if current.is_null() {
        *current = match segment {
            PathSegment::Index(_) => json!([]),
            _ => json!({}),
        };
    }
    match segment {
        PathSegment::Key(key) => {
            let object = current
                .as_object_mut()
                .ok_or_else(|| RewriteError::InvalidJson(format!("expected object at {key}")))?;
            set_json(
                object.entry(key.clone()).or_insert(Value::Null),
                rest,
                value,
            )
        }
        PathSegment::Index(index) => {
            let array = current
                .as_array_mut()
                .ok_or_else(|| RewriteError::InvalidJson("expected array".into()))?;
            if *index > array.len() {
                return Err(RewriteError::InvalidJson(
                    "array index out of bounds".into(),
                ));
            }
            if *index == array.len() {
                array.push(Value::Null);
            }
            set_json(&mut array[*index], rest, value)
        }
        PathSegment::Wildcard => {
            match current {
                Value::Array(items) => {
                    for item in items {
                        set_json(item, rest, value)?;
                    }
                }
                Value::Object(items) => {
                    for item in items.values_mut() {
                        set_json(item, rest, value)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }
}
