//! Claude models behind Antigravity.
//!
//! The Code Assist host serves `claude-*` models by translating the Gemini
//! envelope into an Anthropic Messages call on Vertex, so a request has to
//! satisfy both the Gemini proto the host parses and the Anthropic
//! validation behind it; an Anthropic refusal comes back inside a Gemini
//! `INVALID_ARGUMENT`. Every rule here was probed live against
//! `daily-cloudcode-pa` (2026-09-26) rather than taken from a reference
//! client, and each one names the refusal it avoids.
//!
//! Tools: a declaration carrying `parametersJsonSchema` never reaches the
//! Anthropic side (`tools.0.custom.input_schema: Field required`); only the
//! proto `parameters` does. That proto `Schema` rejects the JSON Schema
//! keywords it does not declare (`$schema`, `$defs`, `$ref`, `const`,
//! `deprecated`, `examples`, a `type` array) with `Unknown name`, and an
//! `anyOf` it accepts is then refused by Anthropic as an invalid schema, so
//! the schema is rewritten into the subset both sides take. Neither side
//! needs a `required` entry or the `VALIDATED` calling mode.
//!
//! Output and thinking: without `maxOutputTokens` the host caps a Claude
//! reply at 8192 tokens (a budget of 8191 passes, 8192 is refused with
//! "`max_tokens` must be greater than `thinking.budget_tokens`"), so the
//! caller's limit is kept rather than stripped as it is for Gemini models,
//! and filled in at the model maximum when thinking is on. Thinking only
//! happens for a positive `thinkingBudget` of at least 1024 (below that is
//! refused); a `thinkingLevel` or the dynamic `-1` budget is silently
//! ignored, so both become an explicit budget. `includeThoughts` changes
//! nothing: a thinking reply always carries its thoughts.
//!
//! History: Anthropic refuses an empty text block (`text.text: Field
//! required`), a conversation that ends on the model's turn ("does not
//! support assistant message prefill"), and a thinking block without its
//! signature (`thinking.signature: Field required`). The signature only
//! counts when it rides the thought part that carries the thinking text:
//! the host streams it as a trailing thought part with empty text, and that
//! part replayed on its own is refused (`thinking.thinking: Field
//! required`), so split pieces are joined back into one part first.

use serde_json::{Map, Value};

/// The routed model is served by Anthropic rather than Gemini.
pub(super) fn is_claude(model: &str) -> bool {
    crate::channels::shared::code_assist::model_id(model).starts_with("claude-")
}

/// The output ceiling the catalogue reports for every Claude model it
/// lists; Anthropic's own ceiling is higher for some, but the catalogue is
/// what the account is entitled to.
const MAX_OUTPUT_TOKENS: u64 = 64_000;
/// Anthropic's smallest thinking budget.
const MIN_THINKING_BUDGET: u64 = 1024;
/// The budget a dynamic (`-1`) or unleveled request gets.
const DEFAULT_THINKING_BUDGET: u64 = 16_384;

/// The output limit a Gemini body (or a whole envelope) asks for, read
/// before the shared sanitizer strips it.
pub(super) fn output_limit(body: &Value) -> Option<u64> {
    let config = body
        .get("generationConfig")
        .or_else(|| body.pointer("/request/generationConfig"))?;
    ["maxOutputTokens", "max_output_tokens"]
        .iter()
        .find_map(|name| config.get(*name))
        .and_then(Value::as_u64)
}

/// The budget a thinking configuration asks for, if it asks for thinking.
fn thinking_budget(thinking: &Map<String, Value>) -> Option<u64> {
    let budget = thinking
        .get("thinkingBudget")
        .or_else(|| thinking.get("thinking_budget"))
        .and_then(Value::as_i64);
    match budget {
        Some(0) => return None,
        Some(budget) if budget > 0 => return Some(budget.unsigned_abs()),
        Some(_) => return Some(DEFAULT_THINKING_BUDGET),
        None => {}
    }
    let level = thinking
        .get("thinkingLevel")
        .or_else(|| thinking.get("thinking_level"))
        .and_then(Value::as_str)?;
    Some(match level.to_ascii_uppercase().as_str() {
        "MINIMAL" => MIN_THINKING_BUDGET,
        "LOW" => 4096,
        "MEDIUM" => DEFAULT_THINKING_BUDGET,
        "HIGH" => 32_768,
        _ => DEFAULT_THINKING_BUDGET,
    })
}

/// Restore the caller's output limit and turn the thinking request into the
/// one budget Anthropic takes, keeping `maxOutputTokens > thinkingBudget`.
pub(super) fn apply_limits(request: &mut Value, requested: Option<u64>) {
    let Some(request) = request.as_object_mut() else {
        return;
    };
    let config = request
        .entry("generationConfig")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(config) = config.as_object_mut() else {
        return;
    };
    let budget = config
        .get("thinkingConfig")
        .and_then(Value::as_object)
        .and_then(thinking_budget);
    let limit = requested.map(|limit| limit.clamp(1, MAX_OUTPUT_TOKENS));
    let budget = budget.and_then(|budget| {
        let ceiling = limit.unwrap_or(MAX_OUTPUT_TOKENS) - 1;
        let budget = budget.clamp(MIN_THINKING_BUDGET, ceiling.max(MIN_THINKING_BUDGET));
        // A limit too small for the smallest budget leaves no room to think.
        (budget <= ceiling).then_some(budget)
    });
    match budget {
        Some(budget) => {
            config.insert(
                "thinkingConfig".into(),
                serde_json::json!({ "thinkingBudget": budget, "includeThoughts": true }),
            );
            config.insert(
                "maxOutputTokens".into(),
                Value::from(limit.unwrap_or(MAX_OUTPUT_TOKENS)),
            );
        }
        None => {
            config.remove("thinkingConfig");
            if let Some(limit) = limit {
                config.insert("maxOutputTokens".into(), Value::from(limit));
            }
        }
    }
    if config.is_empty() {
        request.remove("generationConfig");
    }
}

fn is_thought(part: &Value) -> bool {
    part.get("thought").and_then(Value::as_bool) == Some(true)
}

fn signature(part: &Value) -> Option<&str> {
    part.get("thoughtSignature")
        .and_then(Value::as_str)
        .filter(|signature| !signature.is_empty())
}

fn text(part: &Value) -> Option<&str> {
    part.get("text").and_then(Value::as_str)
}

/// A part that is nothing but (possibly empty) text.
fn bare_text(part: &Value) -> bool {
    part.as_object().is_some_and(|object| {
        object.contains_key("text")
            && object
                .keys()
                .all(|key| matches!(key.as_str(), "text" | "thought" | "thoughtSignature"))
    })
}

/// One model turn's parts with each run of thought text joined into the
/// part that carries its signature; thought left unsigned is dropped.
fn join_thoughts(parts: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(parts.len());
    let mut run: Option<(String, Option<String>)> = None;
    let flush = |run: &mut Option<(String, Option<String>)>, out: &mut Vec<Value>| {
        if let Some((text, Some(signature))) = run.take()
            && !text.is_empty()
        {
            out.push(serde_json::json!({
                "thought": true,
                "text": text,
                "thoughtSignature": signature,
            }));
        }
    };
    for part in parts {
        if is_thought(&part) && bare_text(&part) {
            // A signature closes its run: the next thought starts a new block.
            if run.as_ref().is_some_and(|(_, signed)| signed.is_some()) {
                flush(&mut run, &mut out);
            }
            let (text_so_far, signed) = run.get_or_insert_with(|| (String::new(), None));
            text_so_far.push_str(text(&part).unwrap_or_default());
            if let Some(found) = signature(&part) {
                *signed = Some(found.to_owned());
            }
            continue;
        }
        flush(&mut run, &mut out);
        out.push(part);
    }
    flush(&mut run, &mut out);
    out
}

/// Rewrite the history into the shape Anthropic accepts: signed thinking
/// joined into one part, unsigned thinking and empty text dropped, emptied
/// turns removed, and a trailing model turn (a prefill) cut off.
pub(super) fn tidy_history(request: &mut Value) {
    let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) else {
        return;
    };
    for content in contents.iter_mut() {
        let model = content.get("role").and_then(Value::as_str) == Some("model");
        let Some(parts) = content.get_mut("parts").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut kept = std::mem::take(parts);
        if model {
            kept = join_thoughts(kept);
        } else {
            kept.retain(|part| !is_thought(part));
        }
        kept.retain(|part| !(bare_text(part) && !is_thought(part) && text(part) == Some("")));
        *parts = kept;
    }
    contents.retain(|content| {
        content
            .get("parts")
            .and_then(Value::as_array)
            .is_some_and(|parts| !parts.is_empty())
    });
    while contents
        .last()
        .is_some_and(|content| content.get("role").and_then(Value::as_str) == Some("model"))
    {
        contents.pop();
    }
}

/// Keywords the proto `Schema` declares and Anthropic accepts, kept as they
/// are once their nested schemas are rewritten.
const KEPT: &[&str] = &[
    "type",
    "format",
    "title",
    "description",
    "nullable",
    "enum",
    "default",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "minProperties",
    "maxProperties",
];

/// How deep `$ref` inlining and nesting may go before a node is cut short;
/// a recursive definition would otherwise never end.
const MAX_DEPTH: usize = 24;

/// Rewrite every function declaration of a Claude-bound request to the proto
/// `parameters` field with a schema both sides accept. A response schema has
/// no Anthropic counterpart and is dropped.
pub(super) fn declare_tools(request: &mut Value) {
    let Some(tools) = request.get_mut("tools").and_then(Value::as_array_mut) else {
        return;
    };
    for declaration in tools
        .iter_mut()
        .filter_map(|tool| tool.get_mut("functionDeclarations"))
        .filter_map(Value::as_array_mut)
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        let schema = declaration
            .remove("parametersJsonSchema")
            .or_else(|| declaration.remove("parameters"));
        declaration.remove("response");
        declaration.remove("responseJsonSchema");
        if let Some(schema) = schema {
            let definitions = definitions(&schema);
            declaration.insert("parameters".into(), clean(&schema, &definitions, MAX_DEPTH));
        }
    }
}

/// `$defs` and the older `definitions`, for resolving local `$ref`s.
fn definitions(schema: &Value) -> Map<String, Value> {
    let mut found = Map::new();
    for key in ["definitions", "$defs"] {
        if let Some(defs) = schema.get(key).and_then(Value::as_object) {
            found.extend(defs.clone());
        }
    }
    found
}

fn resolve<'a>(reference: &str, definitions: &'a Map<String, Value>) -> Option<&'a Value> {
    let name = reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))?;
    definitions.get(name)
}

/// The non-null variants of a union, and whether `null` was one of them.
fn variants(schema: &Map<String, Value>) -> Option<(Vec<&Value>, bool)> {
    let union = schema
        .get("anyOf")
        .or_else(|| schema.get("oneOf"))
        .and_then(Value::as_array)?;
    let null = union
        .iter()
        .any(|variant| variant.get("type").and_then(Value::as_str) == Some("null"));
    let rest = union
        .iter()
        .filter(|variant| variant.get("type").and_then(Value::as_str) != Some("null"))
        .collect();
    Some((rest, null))
}

fn clean(schema: &Value, definitions: &Map<String, Value>, depth: usize) -> Value {
    let Some(object) = schema.as_object() else {
        // `true` (anything) and other non-object schemas carry no shape.
        return Value::Object(Map::new());
    };
    if depth == 0 {
        return Value::Object(Map::new());
    }
    if let Some(target) = object
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| resolve(reference, definitions))
    {
        // Sibling keywords of a `$ref` (a description, usually) override the
        // definition's own.
        let mut merged = target.as_object().cloned().unwrap_or_default();
        for (key, value) in object {
            if key != "$ref" {
                merged.insert(key.clone(), value.clone());
            }
        }
        return clean(&Value::Object(merged), definitions, depth - 1);
    }

    let mut out = Map::new();
    let mut nullable = false;
    // A union collapses to its first non-null variant; `null` survives as
    // `nullable`.
    let mut source = object.clone();
    if let Some((rest, null)) = variants(object) {
        nullable |= null;
        if let Some(first) = rest.first().and_then(|variant| variant.as_object()) {
            for (key, value) in first {
                source.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
    // `allOf` members are merged into one object shape.
    if let Some(members) = object.get("allOf").and_then(Value::as_array) {
        for member in members.iter().filter_map(Value::as_object) {
            for (key, value) in member {
                match (key.as_str(), source.get_mut(key)) {
                    ("properties", Some(Value::Object(existing))) => {
                        if let Some(extra) = value.as_object() {
                            for (name, property) in extra {
                                existing
                                    .entry(name.clone())
                                    .or_insert_with(|| property.clone());
                            }
                        }
                    }
                    ("required", Some(Value::Array(existing))) => {
                        if let Some(extra) = value.as_array() {
                            existing.extend(extra.iter().cloned());
                        }
                    }
                    (_, Some(_)) => {}
                    (_, None) => {
                        source.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }

    // A `type` array keeps its first non-null type.
    match source.get("type") {
        Some(Value::Array(types)) => {
            nullable |= types.iter().any(|kind| kind.as_str() == Some("null"));
            if let Some(kind) = types.iter().find(|kind| kind.as_str() != Some("null")) {
                out.insert("type".into(), kind.clone());
            }
        }
        Some(kind @ Value::String(_)) => {
            out.insert("type".into(), kind.clone());
        }
        _ => {}
    }
    for key in KEPT.iter().filter(|key| **key != "type") {
        if let Some(value) = source.get(*key) {
            out.insert((*key).into(), value.clone());
        }
    }
    // `const` is a one-value enum.
    if let Some(value) = source.get("const") {
        out.insert("enum".into(), Value::Array(vec![value.clone()]));
    }
    // The proto enum is a list of strings.
    if let Some(Value::Array(values)) = out.get_mut("enum") {
        let mut stringified = false;
        for value in values.iter_mut() {
            if !value.is_string() {
                let text = match &*value {
                    Value::Null => "null".to_owned(),
                    other => other.to_string(),
                };
                *value = Value::String(text);
                stringified = true;
            }
        }
        if stringified {
            out.insert("type".into(), Value::String("string".into()));
        }
    }
    if nullable {
        out.insert("nullable".into(), Value::Bool(true));
    }

    if let Some(properties) = source.get("properties").and_then(Value::as_object) {
        let cleaned: Map<String, Value> = properties
            .iter()
            .map(|(name, property)| (name.clone(), clean(property, definitions, depth - 1)))
            .collect();
        if let Some(required) = source.get("required").and_then(Value::as_array) {
            let mut kept: Vec<Value> = Vec::new();
            for name in required {
                if name.as_str().is_some_and(|name| cleaned.contains_key(name))
                    && !kept.contains(name)
                {
                    kept.push(name.clone());
                }
            }
            if !kept.is_empty() {
                out.insert("required".into(), Value::Array(kept));
            }
        }
        out.insert("properties".into(), Value::Object(cleaned));
    }
    match source.get("items") {
        Some(Value::Array(tuple)) => {
            if let Some(first) = tuple.first() {
                out.insert("items".into(), clean(first, definitions, depth - 1));
            }
        }
        Some(items) => {
            out.insert("items".into(), clean(items, definitions, depth - 1));
        }
        None => {}
    }
    match source.get("additionalProperties") {
        Some(Value::Bool(allowed)) => {
            out.insert("additionalProperties".into(), Value::Bool(*allowed));
        }
        Some(schema @ Value::Object(_)) => {
            out.insert(
                "additionalProperties".into(),
                clean(schema, definitions, depth - 1),
            );
        }
        _ => {}
    }
    // An untyped node with properties is an object.
    if !out.contains_key("type") && out.contains_key("properties") {
        out.insert("type".into(), Value::String("object".into()));
    }
    Value::Object(out)
}
