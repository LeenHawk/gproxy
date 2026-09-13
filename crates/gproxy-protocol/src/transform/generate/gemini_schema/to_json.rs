use super::{Budget, SchemaLimits};
use crate::{
    transform::{Converted, Report, TransformError},
    wire::gemini::content::{Schema, SchemaType},
};
use serde_json::{Map, Value};

/// Read only declared schema fields. Formal `default` and `example` JSON values
/// retain all their members, including members named `rest`.
pub fn to_json(
    input: &Schema,
    limits: SchemaLimits,
) -> Result<Converted<Map<String, Value>>, TransformError> {
    let mut report = Report::default();
    let value = convert(
        input,
        &mut Budget { limits, nodes: 0 },
        0,
        "schema",
        &mut report,
    )?;
    Ok(Converted { value, report })
}

fn convert(
    input: &Schema,
    budget: &mut Budget,
    depth: usize,
    path: &str,
    report: &mut Report,
) -> Result<Map<String, Value>, TransformError> {
    budget.enter(depth, path)?;
    let mut out = Map::new();
    let type_name = match input.type_ {
        SchemaType::Unspecified => None,
        SchemaType::String => Some("string"),
        SchemaType::Number => Some("number"),
        SchemaType::Integer => Some("integer"),
        SchemaType::Boolean => Some("boolean"),
        SchemaType::Array => Some("array"),
        SchemaType::Object => Some("object"),
        SchemaType::Null => Some("null"),
    };
    if let Some(name) = type_name {
        out.insert("type".into(), name.into());
    } else if input.any_of.as_ref().is_none_or(Vec::is_empty) {
        return Err(TransformError::shape(
            format!("{path}.type"),
            "unspecified type requires an anyOf schema",
        ));
    }
    for (key, value) in [
        ("format", &input.format),
        ("title", &input.title),
        ("description", &input.description),
        ("pattern", &input.pattern),
    ] {
        if let Some(value) = value {
            out.insert(key.into(), value.clone().into());
        }
    }
    for (key, value) in [
        ("minItems", &input.min_items),
        ("maxItems", &input.max_items),
        ("minLength", &input.min_length),
        ("maxLength", &input.max_length),
        ("minProperties", &input.min_properties),
        ("maxProperties", &input.max_properties),
    ] {
        if let Some(value) = value {
            let number = value
                .parse::<i64>()
                .ok()
                .filter(|n| *n >= 0)
                .ok_or_else(|| {
                    TransformError::shape(
                        format!("{path}.{key}"),
                        "expected a nonnegative int64 string",
                    )
                })?;
            out.insert(key.into(), number.into());
        }
    }
    for (key, value) in [("minimum", input.minimum), ("maximum", input.maximum)] {
        if let Some(value) = value {
            let number = serde_json::Number::from_f64(value).ok_or_else(|| {
                TransformError::shape(format!("{path}.{key}"), "bound must be finite")
            })?;
            out.insert(key.into(), Value::Number(number));
        }
    }
    if let Some(values) = &input.r#enum {
        if values.is_empty() {
            return Err(TransformError::shape(
                format!("{path}.enum"),
                "enum must be nonempty",
            ));
        }
        if input.type_ != SchemaType::String {
            return Err(TransformError::shape(
                format!("{path}.enum"),
                "Gemini enum is declared only for STRING schemas",
            ));
        }
        out.insert("enum".into(), serde_json::to_value(values)?);
    }
    if let Some(values) = &input.required {
        out.insert("required".into(), serde_json::to_value(values)?);
    }
    if let Some(values) = &input.properties {
        let mut properties = Map::new();
        for (name, schema) in values {
            properties.insert(
                name.clone(),
                Value::Object(convert(
                    schema,
                    budget,
                    depth + 1,
                    &format!("{path}.properties.{name}"),
                    report,
                )?),
            );
        }
        out.insert("properties".into(), Value::Object(properties));
    }
    if let Some(items) = &input.items {
        out.insert(
            "items".into(),
            Value::Object(convert(
                items,
                budget,
                depth + 1,
                &format!("{path}.items"),
                report,
            )?),
        );
    }
    if let Some(values) = &input.any_of {
        if values.is_empty() {
            return Err(TransformError::shape(
                format!("{path}.anyOf"),
                "anyOf must be nonempty",
            ));
        }
        let mut variants = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            variants.push(Value::Object(convert(
                value,
                budget,
                depth + 1,
                &format!("{path}.anyOf[{index}]"),
                report,
            )?));
        }
        out.insert("anyOf".into(), Value::Array(variants));
    }
    // In OpenAPI 3.0 nullable expands the type, but other constraints (including
    // enum and anyOf) still apply. Do not wrap the whole schema in a null union.
    if input.nullable == Some(true)
        && let Some(name) = type_name.filter(|name| *name != "null")
    {
        out.insert("type".into(), serde_json::json!([name, "null"]));
    }
    if let Some(value) = &input.default {
        out.insert("default".into(), value.clone());
    }
    if let Some(value) = &input.example {
        out.insert("examples".into(), Value::Array(vec![value.clone()]));
    }
    if input.property_ordering.is_some() {
        report.omitted(
            format!("{path}.propertyOrdering"),
            "JSON Schema has no property emission ordering constraint",
        );
    }
    Ok(out)
}
