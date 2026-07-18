//! Kind-agnostic rule applications: dot-path JSON rewrite, generic transform,
//! header injection. All fail-soft: a rule that cannot apply warns and skips.

use bytes::Bytes;
use http::HeaderMap;
use http::header::{HeaderName, HeaderValue};
use serde_json::Value;

use super::compile::{RewriteAction, TransformAction, TransformCfg, TransformLocate};

/// Dot-path rewrite on the body value. Segments index objects by key and
/// arrays by number (e.g. `messages.0.content`). `set` creates missing object
/// parents; `merge` shallow-merges objects; `delete` removes the leaf.
pub fn rewrite(root: &mut Value, path: &str, action: RewriteAction, value_json: Option<&Value>) {
    let segs: Vec<&str> = path.split('.').collect();
    let Some((leaf, parents)) = segs.split_last() else {
        return;
    };
    let create = action == RewriteAction::Set;
    let mut cur = root;
    for seg in parents {
        match descend(cur, seg, create) {
            Some(v) => cur = v,
            None => {
                tracing::warn!(path, "process rewrite: path not found; rule skipped");
                return;
            }
        }
    }
    match action {
        RewriteAction::Set => {
            let Some(v) = value_json else { return };
            match cur {
                Value::Object(map) => {
                    map.insert((*leaf).to_owned(), v.clone());
                }
                Value::Array(arr) => {
                    if let Ok(i) = leaf.parse::<usize>()
                        && i < arr.len()
                    {
                        arr[i] = v.clone();
                    }
                }
                _ => tracing::warn!(path, "process rewrite: parent not a container"),
            }
        }
        RewriteAction::Delete => match cur {
            Value::Object(map) => {
                map.remove(*leaf);
            }
            Value::Array(arr) => {
                if let Ok(i) = leaf.parse::<usize>()
                    && i < arr.len()
                {
                    arr.remove(i);
                }
            }
            _ => {}
        },
        RewriteAction::Merge => {
            let Some(Value::Object(src)) = value_json else {
                tracing::warn!(path, "process merge: value_json must be an object");
                return;
            };
            if let Some(Value::Object(dst)) = descend(cur, leaf, false) {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

fn descend<'a>(cur: &'a mut Value, seg: &str, create: bool) -> Option<&'a mut Value> {
    match cur {
        Value::Object(map) => {
            if create && !map.contains_key(seg) {
                map.insert(seg.to_owned(), Value::Object(serde_json::Map::new()));
            }
            map.get_mut(seg)
        }
        Value::Array(arr) => seg.parse::<usize>().ok().and_then(move |i| arr.get_mut(i)),
        _ => None,
    }
}

/// Apply a structural transform to every JSON value matched by a simple
/// dot-path. Supports `*` over arrays/objects and numeric array indexes.
pub fn transform_value(root: &mut Value, cfg: &TransformCfg) {
    let mut hits = 0usize;
    let mut apply = |value: &mut Value| {
        for action in &cfg.actions {
            apply_transform_action(value, action);
        }
    };
    match &cfg.locate {
        TransformLocate::Path(path) => {
            visit_transform_path(root, path, cfg.limit, &mut hits, &mut apply);
        }
        TransformLocate::Paths(paths) => {
            for path in paths {
                if cfg.limit.is_some_and(|limit| hits >= limit) {
                    break;
                }
                visit_transform_path(root, path, cfg.limit, &mut hits, &mut apply);
            }
        }
        TransformLocate::Match(_) => {}
    }
}

fn visit_transform_path(
    root: &mut Value,
    path: &str,
    limit: Option<usize>,
    hits: &mut usize,
    f: &mut impl FnMut(&mut Value),
) {
    let segs: Vec<&str> = path.split('.').filter(|seg| !seg.is_empty()).collect();
    if segs.is_empty() {
        return;
    }
    visit_path(root, &segs, limit, hits, f);
}

fn visit_path(
    cur: &mut Value,
    segs: &[&str],
    limit: Option<usize>,
    hits: &mut usize,
    f: &mut impl FnMut(&mut Value),
) {
    if limit.is_some_and(|limit| *hits >= limit) {
        return;
    }
    let Some((seg, rest)) = segs.split_first() else {
        *hits += 1;
        f(cur);
        return;
    };

    match (cur, *seg) {
        (Value::Array(arr), "*") => {
            for item in arr {
                visit_path(item, rest, limit, hits, f);
            }
        }
        (Value::Object(map), "*") => {
            for item in map.values_mut() {
                visit_path(item, rest, limit, hits, f);
            }
        }
        (Value::Array(arr), idx) => {
            if let Ok(idx) = idx.parse::<usize>()
                && let Some(item) = arr.get_mut(idx)
            {
                visit_path(item, rest, limit, hits, f);
            }
        }
        (Value::Object(map), key) => {
            if let Some(item) = map.get_mut(key) {
                visit_path(item, rest, limit, hits, f);
            }
        }
        _ => {}
    }
}

fn apply_transform_action(value: &mut Value, action: &TransformAction) {
    match action {
        TransformAction::ReplaceText { from, with } => {
            let Some(text) = value.as_str() else {
                return;
            };
            if from.as_deref().is_none_or(|from| text == from) {
                *value = Value::String(with.clone());
            }
        }
    }
}

/// Apply a regex transform over the serialized body. Zero-copy when nothing
/// matches.
pub fn transform_text(body: Bytes, cfg: &TransformCfg) -> Bytes {
    let TransformLocate::Match(regex) = &cfg.locate else {
        return body;
    };
    let text = String::from_utf8_lossy(&body);
    let mut out: Option<String> = None;
    for action in &cfg.actions {
        let TransformAction::ReplaceText { with, .. } = action;
        let next = match (&out, cfg.limit) {
            (Some(current), Some(limit)) => regex.replacen(current, limit, with.as_str()),
            (Some(current), None) => regex.replace_all(current, with.as_str()),
            (None, Some(limit)) => regex.replacen(&text, limit, with.as_str()),
            (None, None) => regex.replace_all(&text, with.as_str()),
        };
        match next {
            std::borrow::Cow::Borrowed(_) if out.is_none() => {}
            std::borrow::Cow::Borrowed(_) => {}
            std::borrow::Cow::Owned(s) => out = Some(s),
        }
    }
    out.map(Bytes::from).unwrap_or(body)
}

/// Set or merge a request header. `override` replaces; `merge` comma-appends
/// with dedup (for list-valued headers like `anthropic-beta`).
pub fn header(
    headers: &mut HeaderMap,
    name: &HeaderName,
    value: &str,
    mode: super::compile::HeaderMode,
) {
    use super::compile::HeaderMode;
    match mode {
        HeaderMode::Override => match HeaderValue::from_str(value) {
            Ok(v) => {
                headers.insert(name, v);
            }
            Err(_) => tracing::warn!(
                header = %name,
                value,
                "process header rule: invalid header value; rule skipped"
            ),
        },
        HeaderMode::Merge => {
            let merged = match headers.get(name).and_then(|v| v.to_str().ok()) {
                Some(existing) if existing.split(',').any(|t| t.trim() == value) => return,
                Some(existing) => format!("{existing},{value}"),
                None => value.to_owned(),
            };
            match HeaderValue::from_str(&merged) {
                Ok(v) => {
                    headers.insert(name, v);
                }
                Err(_) => tracing::warn!(
                    header = %name,
                    value,
                    "process header rule: invalid merged header value; rule skipped"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::compile::HeaderMode;
    use serde_json::json;

    #[test]
    fn rewrite_set_delete_merge() {
        let mut v = json!({"a": {"b": 1}, "arr": [1, 2]});
        rewrite(&mut v, "a.c.d", RewriteAction::Set, Some(&json!(9)));
        assert_eq!(v["a"]["c"]["d"], 9); // set creates parents
        rewrite(&mut v, "a.nullable", RewriteAction::Set, Some(&Value::Null));
        assert!(v["a"]["nullable"].is_null());
        rewrite(&mut v, "arr.0", RewriteAction::Delete, None);
        assert_eq!(v["arr"], json!([2]));
        rewrite(
            &mut v,
            "a",
            RewriteAction::Merge,
            Some(&json!({"b": 5, "e": 6})),
        );
        assert_eq!(v["a"]["b"], 5);
        assert_eq!(v["a"]["e"], 6);
    }

    #[test]
    fn header_override_and_merge() {
        let beta: HeaderName = "anthropic-beta".parse().unwrap();

        // override replaces existing
        let mut h = HeaderMap::new();
        h.insert(&beta, "old-token".parse().unwrap());
        header(&mut h, &beta, "new-token", HeaderMode::Override);
        assert_eq!(h.get(&beta).unwrap(), "new-token");

        // merge dedups existing token
        let mut h = HeaderMap::new();
        h.insert(&beta, "context-1m".parse().unwrap());
        header(&mut h, &beta, "context-1m", HeaderMode::Merge);
        assert_eq!(h.get(&beta).unwrap(), "context-1m"); // unchanged

        // merge appends new token
        let mut h = HeaderMap::new();
        h.insert(&beta, "context-1m".parse().unwrap());
        header(
            &mut h,
            &beta,
            "interleaved-thinking-2025-05-14",
            HeaderMode::Merge,
        );
        assert_eq!(
            h.get(&beta).unwrap(),
            "context-1m,interleaved-thinking-2025-05-14"
        );
    }
}
