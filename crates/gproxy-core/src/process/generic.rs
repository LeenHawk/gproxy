use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use super::{HeaderMode, RewriteAction, TransformAction, TransformConfig, TransformLocate};

pub(super) fn rewrite(
    root: &mut Value,
    path: &str,
    action: RewriteAction,
    value: Option<&Value>,
) -> bool {
    let segments = path
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some((leaf, parents)) = segments.split_last() else {
        return false;
    };
    let mut current = root;
    for segment in parents {
        let Some(next) = descend(current, segment, action == RewriteAction::Set) else {
            return false;
        };
        current = next;
    }
    match (action, current) {
        (RewriteAction::Set, Value::Object(map)) => {
            let value = value.expect("compiled set value").clone();
            if map.get(*leaf) == Some(&value) {
                false
            } else {
                map.insert((*leaf).into(), value);
                true
            }
        }
        (RewriteAction::Set, Value::Array(array)) => leaf
            .parse::<usize>()
            .ok()
            .filter(|index| *index < array.len())
            .is_some_and(|index| {
                let value = value.expect("compiled set value").clone();
                if array[index] == value {
                    false
                } else {
                    array[index] = value;
                    true
                }
            }),
        (RewriteAction::Set, _) => false,
        (RewriteAction::Delete, Value::Object(map)) => map.remove(*leaf).is_some(),
        (RewriteAction::Delete, Value::Array(array)) => leaf
            .parse::<usize>()
            .ok()
            .filter(|index| *index < array.len())
            .is_some_and(|index| {
                array.remove(index);
                true
            }),
        (RewriteAction::Delete, _) => false,
        (RewriteAction::Merge, Value::Object(map)) => {
            let Some(Value::Object(source)) = value else {
                return false;
            };
            let Some(Value::Object(target)) = map.get_mut(*leaf) else {
                return false;
            };
            let mut changed = false;
            for (key, value) in source {
                if target.get(key) != Some(value) {
                    target.insert(key.clone(), value.clone());
                    changed = true;
                }
            }
            changed
        }
        (RewriteAction::Merge, _) => false,
    }
}

fn descend<'a>(value: &'a mut Value, segment: &str, create: bool) -> Option<&'a mut Value> {
    match value {
        Value::Object(map) => {
            if create && !map.contains_key(segment) {
                map.insert(segment.into(), Value::Object(Default::default()));
            }
            map.get_mut(segment)
        }
        Value::Array(array) => segment
            .parse::<usize>()
            .ok()
            .and_then(|index| array.get_mut(index)),
        _ => None,
    }
}

pub(super) fn transform_value(root: &mut Value, config: &TransformConfig) -> bool {
    let mut hits = 0;
    let mut changed = false;
    let paths = match &config.locate {
        TransformLocate::Path(path) => std::slice::from_ref(path),
        TransformLocate::Paths(paths) => paths.as_slice(),
        TransformLocate::Match(_) => return false,
    };
    for path in paths {
        let segments = path
            .split('.')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        visit(root, &segments, config, &mut hits, &mut changed);
        if config.limit.is_some_and(|limit| hits >= limit) {
            break;
        }
    }
    changed
}

fn visit(
    value: &mut Value,
    path: &[&str],
    config: &TransformConfig,
    hits: &mut usize,
    changed: &mut bool,
) {
    if config.limit.is_some_and(|limit| *hits >= limit) {
        return;
    }
    let Some((segment, rest)) = path.split_first() else {
        *hits += 1;
        for action in &config.actions {
            *changed |= transform_action(value, action);
        }
        return;
    };
    match (value, *segment) {
        (Value::Array(array), "*") => {
            for value in array {
                visit(value, rest, config, hits, changed);
            }
        }
        (Value::Object(map), "*") => {
            for value in map.values_mut() {
                visit(value, rest, config, hits, changed);
            }
        }
        (Value::Array(array), index) => {
            if let Some(value) = index
                .parse::<usize>()
                .ok()
                .and_then(|index| array.get_mut(index))
            {
                visit(value, rest, config, hits, changed);
            }
        }
        (Value::Object(map), key) => {
            if let Some(value) = map.get_mut(key) {
                visit(value, rest, config, hits, changed);
            }
        }
        _ => {}
    }
}

fn transform_action(value: &mut Value, action: &TransformAction) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let replaced = match action {
        TransformAction::ReplaceText { from, with }
            if from.as_deref().is_none_or(|from| from == text) =>
        {
            Some(with.clone())
        }
        TransformAction::ReplaceText { .. } => None,
        TransformAction::ReplaceRegex { regex, with } => {
            match regex.replace_all(text, with.as_str()) {
                std::borrow::Cow::Owned(value) => Some(value),
                std::borrow::Cow::Borrowed(_) => None,
            }
        }
    };
    replaced.is_some_and(|replaced| {
        *value = Value::String(replaced);
        true
    })
}

pub(super) fn transform_text(body: Bytes, config: &TransformConfig) -> Bytes {
    let TransformLocate::Match(regex) = &config.locate else {
        return body;
    };
    let text = String::from_utf8_lossy(&body);
    let mut current: Option<String> = None;
    for action in &config.actions {
        let TransformAction::ReplaceText { with, .. } = action else {
            continue;
        };
        let source = current.as_deref().unwrap_or(&text);
        let next = config.limit.map_or_else(
            || regex.replace_all(source, with),
            |limit| regex.replacen(source, limit, with),
        );
        if let std::borrow::Cow::Owned(value) = next {
            current = Some(value);
        }
    }
    current.map(Bytes::from).unwrap_or(body)
}

pub(super) fn header(
    headers: &mut HeaderMap,
    name: &HeaderName,
    value: &str,
    mode: HeaderMode,
) -> bool {
    let merged = match mode {
        HeaderMode::Override => value.to_owned(),
        HeaderMode::Merge => match headers.get(name).and_then(|value| value.to_str().ok()) {
            Some(existing) if existing.split(',').any(|item| item.trim() == value) => return false,
            Some(existing) => format!("{existing},{value}"),
            None => value.to_owned(),
        },
    };
    let Ok(value) = HeaderValue::from_str(&merged) else {
        return false;
    };
    if headers.get(name) == Some(&value) {
        false
    } else {
        headers.insert(name, value);
        true
    }
}
