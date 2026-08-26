use regex::RegexBuilder;
use serde::Deserialize;
use serde_json::Value;

use super::types::*;

pub fn compile_all(specs: &[RuleSpec]) -> Result<Vec<CompiledRule>, String> {
    let mut rules = specs
        .iter()
        .filter(|spec| spec.enabled)
        .map(compile)
        .collect::<Result<Vec<_>, _>>()?;
    rules.sort_by_key(|rule| rule.sort_order);
    order_for_apply(&mut rules);
    Ok(rules)
}

pub fn order_for_apply(rules: &mut [CompiledRule]) {
    rules.sort_by_key(|rule| rule.config.kind().rank());
}

pub fn compile(spec: &RuleSpec) -> Result<CompiledRule, String> {
    let kind = RuleKind::from_id(&spec.kind)
        .ok_or_else(|| format!("unknown process rule kind `{}`", spec.kind))?;
    let config = parse_config(kind, spec.config.clone())?;
    let operations = spec
        .filter_operations
        .as_ref()
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    gproxy_protocol::Operation::from_id(value)
                        .ok_or_else(|| format!("unknown filter operation `{value}`"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let header_pattern = spec
        .filter_header_pattern
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|pattern| {
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| format!("invalid header filter: {error}"))
        })
        .transpose()?;
    let model_pattern = spec
        .filter_model_pattern
        .as_deref()
        .map(glob_regex)
        .transpose()?;
    Ok(CompiledRule {
        id: spec.id,
        config,
        model_pattern,
        operations,
        header_pattern,
        sort_order: spec.sort_order,
    })
}

fn glob_regex(pattern: &str) -> Result<regex::Regex, String> {
    let mut source = String::from("^");
    for character in pattern.chars() {
        match character {
            '*' => source.push_str(".*"),
            '?' => source.push('.'),
            other => source.push_str(&regex::escape(&other.to_string())),
        }
    }
    source.push('$');
    regex::Regex::new(&source).map_err(|error| error.to_string())
}

fn parse_config(kind: RuleKind, value: Value) -> Result<RuleConfig, String> {
    match kind {
        RuleKind::SystemText => {
            #[derive(Deserialize)]
            struct Raw {
                text: String,
                #[serde(default)]
                position: Position,
            }
            #[derive(Deserialize, Default)]
            #[serde(rename_all = "snake_case")]
            enum Position {
                #[default]
                Prepend,
                Append,
            }
            let raw: Raw = parse(value)?;
            Ok(RuleConfig::SystemText {
                text: raw.text,
                position: match raw.position {
                    Position::Prepend => TextPosition::Prepend,
                    Position::Append => TextPosition::Append,
                },
            })
        }
        RuleKind::CacheBreakpoint => {
            #[derive(Deserialize)]
            struct Raw {
                target: String,
                index: Option<i64>,
                ttl: Option<String>,
            }
            let raw: Raw = parse(value)?;
            Ok(RuleConfig::CacheBreakpoint(CacheBreakpointConfig {
                target: raw.target,
                index: raw.index,
                ttl: raw.ttl,
            }))
        }
        RuleKind::Rewrite => rewrite(value),
        RuleKind::Transform => transform(value),
        RuleKind::Header => header(value),
    }
}

fn rewrite(value: Value) -> Result<RuleConfig, String> {
    #[derive(Deserialize)]
    struct Raw {
        path: String,
        action: String,
        #[serde(default)]
        value: Value,
    }
    let raw: Raw = parse(value)?;
    let action = match raw.action.as_str() {
        "set" => RewriteAction::Set,
        "delete" => RewriteAction::Delete,
        "merge" => RewriteAction::Merge,
        _ => return Err(format!("unknown rewrite action `{}`", raw.action)),
    };
    let value = (action != RewriteAction::Delete).then_some(raw.value);
    Ok(RuleConfig::Rewrite {
        path: raw.path,
        action,
        value,
    })
}

fn transform(value: Value) -> Result<RuleConfig, String> {
    #[derive(Deserialize)]
    struct Locate {
        path: Option<String>,
        paths: Option<Vec<String>>,
        #[serde(rename = "match")]
        match_: Option<String>,
    }
    #[derive(Deserialize)]
    struct Action {
        op: String,
        from: Option<String>,
        pattern: Option<String>,
        with: Option<String>,
        to: Option<String>,
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        phase: String,
        locate: Locate,
        actions: Vec<Action>,
        limit: Option<usize>,
    }
    let raw: Raw = parse(value)?;
    let phase = match raw.phase.as_str() {
        "" | "request" => TransformPhase::Request,
        "response" => TransformPhase::Response,
        "both" => TransformPhase::Both,
        other => return Err(format!("unknown transform phase `{other}`")),
    };
    let locate = match (raw.locate.path, raw.locate.paths, raw.locate.match_) {
        (Some(path), None, None) => TransformLocate::Path(path),
        (None, Some(paths), None) if !paths.is_empty() => TransformLocate::Paths(paths),
        (None, None, Some(pattern)) => {
            TransformLocate::Match(regex::Regex::new(&pattern).map_err(|error| error.to_string())?)
        }
        _ => return Err("transform locate must contain exactly one non-empty selector".into()),
    };
    let actions = raw
        .actions
        .into_iter()
        .map(|action| match action.op.as_str() {
            "replace_text" => Ok(TransformAction::ReplaceText {
                from: action.from,
                with: action
                    .with
                    .or(action.to)
                    .ok_or("replace_text requires with")?,
            }),
            "replace_regex" => Ok(TransformAction::ReplaceRegex {
                regex: regex::Regex::new(
                    action
                        .pattern
                        .as_deref()
                        .ok_or("replace_regex requires pattern")?,
                )
                .map_err(|error| error.to_string())?,
                with: action
                    .with
                    .or(action.to)
                    .ok_or("replace_regex requires with")?,
            }),
            other => Err(format!("unknown transform action `{other}`")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if actions.is_empty() {
        return Err("transform requires at least one action".into());
    }
    if matches!(locate, TransformLocate::Match(_))
        && actions
            .iter()
            .any(|action| matches!(action, TransformAction::ReplaceRegex { .. }))
    {
        return Err(
            "match locate only supports replace_text for incremental response frames".into(),
        );
    }
    Ok(RuleConfig::Transform(TransformConfig {
        phase,
        locate,
        actions,
        limit: raw.limit,
    }))
}

fn header(value: Value) -> Result<RuleConfig, String> {
    #[derive(Deserialize)]
    struct Raw {
        name: String,
        value: String,
        #[serde(default)]
        mode: String,
    }
    let raw: Raw = parse(value)?;
    let mode = match raw.mode.as_str() {
        "" | "override" => HeaderMode::Override,
        "merge" => HeaderMode::Merge,
        other => return Err(format!("unknown header mode `{other}`")),
    };
    Ok(RuleConfig::Header {
        name: raw
            .name
            .parse()
            .map_err(|error| format!("invalid header name: {error}"))?,
        value: raw.value,
        mode,
    })
}

fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}
