//! JSON Schema lowered to the proto `Schema` subset the JSPB wire carries.
//!
//! A function declaration has one schema slot and so does the generation
//! config; neither has a raw JSON Schema counterpart. Requests translated
//! from OpenAI or Claude carry `parametersJsonSchema` and
//! `responseJsonSchema`, so those are rewritten here: local `$ref`s are
//! inlined, a `type` array becomes one type plus `nullable`, `const` becomes
//! a one-value enum, and a union with `null` loses the null variant. Keywords
//! the proto does not declare are left for the encoder to skip.

use serde_json::{Map, Value};

/// Keywords copied as they are.
const KEPT: &[&str] = &[
    "format",
    "description",
    "nullable",
    "enum",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "minProperties",
    "maxProperties",
    "propertyOrdering",
    "example",
];

/// How deep `$ref` inlining and nesting may go before a node is cut short;
/// a recursive definition would otherwise never end.
const MAX_DEPTH: usize = 24;

pub(super) fn lower(schema: &Value) -> Value {
    let mut definitions = Map::new();
    for key in ["definitions", "$defs"] {
        if let Some(found) = schema.get(key).and_then(Value::as_object) {
            definitions.extend(found.clone());
        }
    }
    node(schema, &definitions, MAX_DEPTH)
}

fn is_null(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("null")
}

fn node(schema: &Value, definitions: &Map<String, Value>, depth: usize) -> Value {
    // `true` (anything) and other non-object schemas carry no shape.
    let Some(object) = schema.as_object().filter(|_| depth > 0) else {
        return Value::Object(Map::new());
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let target = reference
            .strip_prefix("#/$defs/")
            .or_else(|| reference.strip_prefix("#/definitions/"))
            .and_then(|name| definitions.get(name));
        // Sibling keywords of a `$ref` (a description, usually) override the
        // definition's own; an unresolved reference leaves only the siblings.
        let mut merged = target
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, value) in object {
            if key != "$ref" {
                merged.insert(key.clone(), value.clone());
            }
        }
        return node(&Value::Object(merged), definitions, depth - 1);
    }

    let mut out = Map::new();
    let mut nullable = false;
    match object.get("type") {
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
    for key in KEPT {
        if let Some(value) = object.get(*key) {
            out.insert((*key).into(), value.clone());
        }
    }
    if let Some(first) = object
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|examples| examples.first())
    {
        out.entry("example").or_insert_with(|| first.clone());
    }
    // `const` is a one-value enum.
    if let Some(value) = object.get("const") {
        out.insert("enum".into(), Value::Array(vec![value.clone()]));
    }
    // The proto enum is a list of strings.
    if let Some(Value::Array(values)) = out.get_mut("enum") {
        let mut stringified = false;
        for value in values.iter_mut().filter(|value| !value.is_string()) {
            *value = Value::String(match &*value {
                Value::Null => "null".to_owned(),
                other => other.to_string(),
            });
            stringified = true;
        }
        if stringified {
            out.insert("type".into(), Value::String("string".into()));
        }
    }

    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        let lowered: Map<String, Value> = properties
            .iter()
            .map(|(name, property)| (name.clone(), node(property, definitions, depth - 1)))
            .collect();
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            let mut kept: Vec<Value> = Vec::new();
            for name in required {
                if name.as_str().is_some_and(|name| lowered.contains_key(name))
                    && !kept.contains(name)
                {
                    kept.push(name.clone());
                }
            }
            if !kept.is_empty() {
                out.insert("required".into(), Value::Array(kept));
            }
        }
        out.insert("properties".into(), Value::Object(lowered));
        // An untyped node with properties is an object.
        out.entry("type")
            .or_insert_with(|| Value::String("object".into()));
    }
    match object.get("items") {
        Some(Value::Array(tuple)) => {
            if let Some(first) = tuple.first() {
                out.insert("items".into(), node(first, definitions, depth - 1));
            }
        }
        Some(items) => {
            out.insert("items".into(), node(items, definitions, depth - 1));
        }
        None => {}
    }
    for key in ["anyOf", "oneOf"] {
        let Some(union) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        nullable |= union.iter().any(is_null);
        let mut variants: Vec<Value> = union
            .iter()
            .filter(|variant| !is_null(variant))
            .map(|variant| node(variant, definitions, depth - 1))
            .collect();
        if variants.len() > 1 {
            out.insert(key.into(), Value::Array(variants));
        } else if let Some(Value::Object(only)) = variants.pop() {
            // One variant left is the node itself.
            for (name, value) in only {
                out.entry(name).or_insert(value);
            }
        }
    }
    if let Some(members) = object.get("allOf").and_then(Value::as_array) {
        out.insert(
            "allOf".into(),
            members
                .iter()
                .map(|member| node(member, definitions, depth - 1))
                .collect(),
        );
    }
    if let Some(negated) = object.get("not") {
        out.insert("not".into(), node(negated, definitions, depth - 1));
    }
    if nullable {
        out.insert("nullable".into(), Value::Bool(true));
    }
    Value::Object(out)
}
