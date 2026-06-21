//! Kind-agnostic rule applications: dot-path JSON rewrite, body sanitize,
//! header injection. All fail-soft: a rule that cannot apply warns and skips.

use bytes::Bytes;
use http::HeaderMap;
use http::header::{HeaderName, HeaderValue};
use regex::Regex;
use serde_json::Value;

use super::compile::RewriteAction;

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

/// Regex replace over the serialized body. Zero-copy when nothing matches.
pub fn sanitize(body: Bytes, regex: &Regex, replacement: &str) -> Bytes {
    let text = String::from_utf8_lossy(&body);
    match regex.replace_all(&text, replacement) {
        std::borrow::Cow::Borrowed(_) => body,
        std::borrow::Cow::Owned(s) => Bytes::from(s),
    }
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
