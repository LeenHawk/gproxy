use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single JSON path rewrite rule applied to a request body.
///
/// The `path` field uses dot-notation to address nested keys (e.g. `"a.b.c"`).
/// The `action` determines whether to set a new value or remove the key.
/// The optional `filter` restricts when the rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteRule {
    pub path: String,
    pub action: RewriteAction,
    pub filter: Option<RewriteFilter>,
}

/// What to do at the addressed path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RewriteAction {
    Set(serde_json::Value),
    Remove,
}

/// Restricts a `RewriteRule` to a subset of requests.
///
/// All specified dimensions must match (AND logic).  A `None` dimension
/// matches everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewriteFilter {
    /// Glob pattern matched against the request model name.
    pub model_pattern: Option<String>,
    /// Allowlist of operation families.
    pub operations: Option<Vec<OperationFamily>>,
    /// Allowlist of protocol kinds.
    pub protocols: Option<Vec<ProtocolKind>>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Apply a slice of rewrite rules to a JSON request body in order.
///
/// Rules are applied sequentially; later rules can overwrite earlier ones.
/// Non-object bodies and empty rule slices are silently ignored.
pub fn apply_rewrite_rules(
    body: &mut Value,
    rules: &[RewriteRule],
    model: Option<&str>,
    operation: OperationFamily,
    protocol: ProtocolKind,
) {
    if rules.is_empty() {
        return;
    }
    if !body.is_object() {
        return;
    }

    for rule in rules {
        // Check filter before doing any path work.
        if let Some(filter) = &rule.filter {
            if !matches_filter(filter, model, operation, protocol) {
                continue;
            }
        }

        let segments: Vec<&str> = rule.path.split('.').collect();
        if segments.is_empty() {
            continue;
        }

        match &rule.action {
            RewriteAction::Set(value) => set_path(body, &segments, value.clone()),
            RewriteAction::Remove => remove_path(body, &segments),
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Walk `segments` into `body`, auto-creating (or overwriting) intermediate
/// objects as needed, then insert `value` at the leaf.
fn set_path(body: &mut Value, segments: &[&str], value: Value) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        if let Some(map) = body.as_object_mut() {
            map.insert(segments[0].to_string(), value);
        }
        return;
    }

    // Ensure the next node exists and is an object.
    let key = segments[0];
    if let Some(map) = body.as_object_mut() {
        let next = map.entry(key.to_string()).or_insert_with(|| Value::Object(Default::default()));
        // If the existing value is not an object, overwrite it.
        if !next.is_object() {
            *next = Value::Object(Default::default());
        }
        set_path(next, &segments[1..], value);
    }
}

/// Walk to the parent of `segments[-1]` and remove the leaf key.
/// Silently does nothing if any part of the path is missing.
fn remove_path(body: &mut Value, segments: &[&str]) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        if let Some(map) = body.as_object_mut() {
            map.remove(segments[0]);
        }
        return;
    }

    let key = segments[0];
    if let Some(map) = body.as_object_mut() {
        if let Some(next) = map.get_mut(key) {
            remove_path(next, &segments[1..]);
        }
    }
}

/// Returns `true` iff all specified filter dimensions match.
fn matches_filter(
    filter: &RewriteFilter,
    model: Option<&str>,
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> bool {
    if let Some(pattern) = &filter.model_pattern {
        let model_str = model.unwrap_or("");
        if !glob_match(pattern, model_str) {
            return false;
        }
    }

    if let Some(ops) = &filter.operations {
        if !ops.contains(&operation) {
            return false;
        }
    }

    if let Some(protos) = &filter.protocols {
        if !protos.contains(&protocol) {
            return false;
        }
    }

    true
}

/// Minimal glob matching supporting `*` (any chars) and `?` (exactly one char).
///
/// Uses a two-pointer backtracking algorithm — no allocations.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            // Backtrack: the star consumes one more character.
            star_ti += 1;
            ti = star_ti;
            pi = star_pi + 1;
        } else {
            return false;
        }
    }

    // Consume any trailing stars.
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn op() -> OperationFamily {
        OperationFamily::GenerateContent
    }
    fn proto() -> ProtocolKind {
        ProtocolKind::OpenAiChatCompletion
    }

    fn rule(path: &str, action: RewriteAction) -> RewriteRule {
        RewriteRule {
            path: path.to_string(),
            action,
            filter: None,
        }
    }

    fn filtered_rule(path: &str, action: RewriteAction, filter: RewriteFilter) -> RewriteRule {
        RewriteRule {
            path: path.to_string(),
            action,
            filter: Some(filter),
        }
    }

    #[test]
    fn set_scalar_top_level() {
        let mut body = json!({"temperature": 1.0});
        apply_rewrite_rules(
            &mut body,
            &[rule("temperature", RewriteAction::Set(json!(0.7)))],
            Some("gpt-4o"),
            op(),
            proto(),
        );
        assert_eq!(body["temperature"], json!(0.7));
    }

    #[test]
    fn set_nested_parents_exist() {
        let mut body = json!({"a": {"b": {}}});
        apply_rewrite_rules(
            &mut body,
            &[rule("a.b.c", RewriteAction::Set(json!("hello")))],
            None,
            op(),
            proto(),
        );
        assert_eq!(body["a"]["b"]["c"], json!("hello"));
    }

    #[test]
    fn set_nested_parents_missing() {
        let mut body = json!({});
        apply_rewrite_rules(
            &mut body,
            &[rule("a.b.c", RewriteAction::Set(json!(true)))],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, json!({"a": {"b": {"c": true}}}));
    }

    #[test]
    fn set_object_value() {
        let mut body = json!({});
        apply_rewrite_rules(
            &mut body,
            &[rule(
                "metadata",
                RewriteAction::Set(json!({"source": "gproxy"})),
            )],
            None,
            op(),
            proto(),
        );
        assert_eq!(body["metadata"], json!({"source": "gproxy"}));
    }

    #[test]
    fn set_array_value() {
        let mut body = json!({});
        apply_rewrite_rules(
            &mut body,
            &[rule(
                "stop",
                RewriteAction::Set(json!(["END", "STOP"])),
            )],
            None,
            op(),
            proto(),
        );
        assert_eq!(body["stop"], json!(["END", "STOP"]));
    }

    #[test]
    fn set_null_value() {
        let mut body = json!({"user": "bob"});
        apply_rewrite_rules(
            &mut body,
            &[rule("user", RewriteAction::Set(json!(null)))],
            None,
            op(),
            proto(),
        );
        assert_eq!(body["user"], json!(null));
    }

    #[test]
    fn set_overwrites_non_object_intermediate() {
        let mut body = json!({"a": "string_value"});
        apply_rewrite_rules(
            &mut body,
            &[rule("a.b.c", RewriteAction::Set(json!(42)))],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, json!({"a": {"b": {"c": 42}}}));
    }

    #[test]
    fn remove_existing_top_level() {
        let mut body = json!({"temperature": 1.0, "model": "x"});
        apply_rewrite_rules(
            &mut body,
            &[rule("temperature", RewriteAction::Remove)],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, json!({"model": "x"}));
    }

    #[test]
    fn remove_nested() {
        let mut body = json!({"a": {"b": {"c": 1, "d": 2}}});
        apply_rewrite_rules(
            &mut body,
            &[rule("a.b.c", RewriteAction::Remove)],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, json!({"a": {"b": {"d": 2}}}));
    }

    #[test]
    fn remove_nonexistent_silent() {
        let mut body = json!({"a": 1});
        let original = body.clone();
        apply_rewrite_rules(
            &mut body,
            &[rule("x.y.z", RewriteAction::Remove)],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, original);
    }

    #[test]
    fn filter_model_pattern_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            model_pattern: Some("gpt-4*".into()),
            ..Default::default()
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            Some("gpt-4o"),
            op(),
            proto(),
        );
        assert_eq!(body["temperature"], json!(0.5));
    }

    #[test]
    fn filter_model_pattern_no_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            model_pattern: Some("gpt-4*".into()),
            ..Default::default()
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            Some("claude-3-opus"),
            op(),
            proto(),
        );
        assert_eq!(body["temperature"], json!(1.0));
    }

    #[test]
    fn filter_operation_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            operations: Some(vec![OperationFamily::GenerateContent]),
            ..Default::default()
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            None,
            OperationFamily::GenerateContent,
            proto(),
        );
        assert_eq!(body["temperature"], json!(0.5));
    }

    #[test]
    fn filter_operation_no_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            operations: Some(vec![OperationFamily::GenerateContent]),
            ..Default::default()
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            None,
            OperationFamily::ModelList,
            proto(),
        );
        assert_eq!(body["temperature"], json!(1.0));
    }

    #[test]
    fn filter_protocol_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            protocols: Some(vec![ProtocolKind::OpenAiChatCompletion]),
            ..Default::default()
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            None,
            op(),
            ProtocolKind::OpenAiChatCompletion,
        );
        assert_eq!(body["temperature"], json!(0.5));
    }

    #[test]
    fn filter_and_logic_all_must_match() {
        let mut body = json!({"temperature": 1.0});
        let filter = RewriteFilter {
            model_pattern: Some("gpt-4*".into()),
            operations: Some(vec![OperationFamily::GenerateContent]),
            protocols: None,
        };
        apply_rewrite_rules(
            &mut body,
            &[filtered_rule(
                "temperature",
                RewriteAction::Set(json!(0.5)),
                filter,
            )],
            Some("gpt-4o"),
            OperationFamily::ModelList,
            proto(),
        );
        assert_eq!(body["temperature"], json!(1.0));
    }

    #[test]
    fn multiple_rules_sequential() {
        let mut body = json!({"temperature": 1.0, "top_p": 0.9});
        let rules = vec![
            rule("temperature", RewriteAction::Set(json!(0.7))),
            rule("top_p", RewriteAction::Remove),
        ];
        apply_rewrite_rules(&mut body, &rules, None, op(), proto());
        assert_eq!(body, json!({"temperature": 0.7}));
    }

    #[test]
    fn later_rule_overwrites_earlier() {
        let mut body = json!({});
        let rules = vec![
            rule("temperature", RewriteAction::Set(json!(0.5))),
            rule("temperature", RewriteAction::Set(json!(0.9))),
        ];
        apply_rewrite_rules(&mut body, &rules, None, op(), proto());
        assert_eq!(body["temperature"], json!(0.9));
    }

    #[test]
    fn non_object_body_skipped() {
        let mut body = json!([1, 2, 3]);
        let original = body.clone();
        apply_rewrite_rules(
            &mut body,
            &[rule("temperature", RewriteAction::Set(json!(0.7)))],
            None,
            op(),
            proto(),
        );
        assert_eq!(body, original);
    }

    #[test]
    fn empty_rules_noop() {
        let mut body = json!({"temperature": 1.0});
        let original = body.clone();
        apply_rewrite_rules(&mut body, &[], None, op(), proto());
        assert_eq!(body, original);
    }

    #[test]
    fn glob_star_suffix() {
        assert!(super::glob_match("gpt-4*", "gpt-4o"));
        assert!(super::glob_match("gpt-4*", "gpt-4"));
        assert!(!super::glob_match("gpt-4*", "gpt-3.5-turbo"));
    }

    #[test]
    fn glob_star_prefix() {
        assert!(super::glob_match("*-turbo", "gpt-3.5-turbo"));
        assert!(!super::glob_match("*-turbo", "gpt-4o"));
    }

    #[test]
    fn glob_star_middle() {
        assert!(super::glob_match("claude-*-opus", "claude-3-opus"));
        assert!(!super::glob_match("claude-*-opus", "claude-3-sonnet"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(super::glob_match("gpt-?o", "gpt-4o"));
        assert!(!super::glob_match("gpt-?o", "gpt-4op"));
    }

    #[test]
    fn glob_exact() {
        assert!(super::glob_match("gpt-4o", "gpt-4o"));
        assert!(!super::glob_match("gpt-4o", "gpt-4o-mini"));
    }

    #[test]
    fn serde_roundtrip_set() {
        let rule = RewriteRule {
            path: "temperature".into(),
            action: RewriteAction::Set(json!(0.7)),
            filter: None,
        };
        let json_str = serde_json::to_string(&rule).unwrap();
        let back: RewriteRule = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.path, "temperature");
        assert!(matches!(back.action, RewriteAction::Set(v) if v == json!(0.7)));
    }

    #[test]
    fn serde_roundtrip_remove() {
        let rule = RewriteRule {
            path: "stream_options".into(),
            action: RewriteAction::Remove,
            filter: Some(RewriteFilter {
                model_pattern: Some("gpt-4*".into()),
                operations: None,
                protocols: None,
            }),
        };
        let json_str = serde_json::to_string(&rule).unwrap();
        let back: RewriteRule = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.path, "stream_options");
        assert!(matches!(back.action, RewriteAction::Remove));
        assert_eq!(back.filter.unwrap().model_pattern, Some("gpt-4*".into()));
    }
}
