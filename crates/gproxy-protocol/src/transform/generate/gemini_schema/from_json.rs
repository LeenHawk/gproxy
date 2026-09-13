use super::{Budget, SchemaLimits};
use crate::{
    transform::{Converted, Report, TransformError},
    wire::gemini::content::{Schema, SchemaType},
};
use serde_json::{Map, Value};

/// Convert the exact typed Gemini subset. Unknown validation keywords fail;
/// callers can choose an endpoint's raw JSON Schema field where supported.
pub fn from_json(
    input: &Map<String, Value>,
    limits: SchemaLimits,
) -> Result<Converted<Schema>, TransformError> {
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
    input: &Map<String, Value>,
    budget: &mut Budget,
    depth: usize,
    path: &str,
    report: &mut Report,
) -> Result<Schema, TransformError> {
    budget.enter(depth, path)?;
    let (kind, nullable) = match input.get("type") {
        Some(Value::String(name)) => (schema_type(name, path)?, false),
        Some(Value::Array(names)) => {
            if names.len() != 2 || !names.iter().any(|name| name == "null") {
                return Err(TransformError::unsupported(
                    format!("{path}.type"),
                    "typed Gemini supports only a single type with optional null",
                ));
            }
            let name = names
                .iter()
                .find(|name| *name != "null")
                .and_then(Value::as_str)
                .ok_or_else(|| TransformError::shape(path, "invalid type union"))?;
            (schema_type(name, path)?, true)
        }
        None if input.contains_key("anyOf") => (SchemaType::Unspecified, false),
        None => {
            return Err(TransformError::unsupported(
                format!("{path}.type"),
                "Gemini typed schema requires a type or anyOf",
            ));
        }
        Some(_) => {
            return Err(TransformError::shape(
                format!("{path}.type"),
                "invalid JSON Schema type",
            ));
        }
    };
    let mut out = Schema::builder(kind).build();
    if nullable {
        out.nullable = Some(true);
    }
    for (key, value) in input {
        let field = format!("{path}.{key}");
        match key.as_str() {
            "type" => {}
            "format" => out.format = Some(decode(value, &field)?),
            "title" => out.title = Some(decode(value, &field)?),
            "description" => out.description = Some(decode(value, &field)?),
            "pattern" => out.pattern = Some(decode(value, &field)?),
            "required" => out.required = Some(decode(value, &field)?),
            "enum" => {
                if out.type_ != SchemaType::String {
                    return Err(TransformError::unsupported(
                        field,
                        "typed Gemini enum supports only strings",
                    ));
                }
                let values: Vec<String> = decode(value, &field)?;
                if values.is_empty() {
                    return Err(TransformError::shape(&field, "enum must be nonempty"));
                }
                out.r#enum = Some(values);
            }
            "minItems" => out.min_items = Some(bound(value, &field)?),
            "maxItems" => out.max_items = Some(bound(value, &field)?),
            "minLength" => out.min_length = Some(bound(value, &field)?),
            "maxLength" => out.max_length = Some(bound(value, &field)?),
            "minProperties" => out.min_properties = Some(bound(value, &field)?),
            "maxProperties" => out.max_properties = Some(bound(value, &field)?),
            "minimum" => out.minimum = Some(number(value, &field)?),
            "maximum" => out.maximum = Some(number(value, &field)?),
            "default" => out.default = Some(value.clone()),
            "examples" => {
                let examples = value
                    .as_array()
                    .ok_or_else(|| TransformError::shape(&field, "examples must be an array"))?;
                out.example = examples.first().cloned();
                if examples.len() > 1 {
                    report.omitted(field, "Gemini carries only the first example");
                }
            }
            "properties" => {
                let values = object(value, &field)?;
                let mut properties = std::collections::BTreeMap::new();
                for (name, value) in values {
                    let child = format!("{field}.{name}");
                    properties.insert(
                        name.clone(),
                        Box::new(convert(
                            object(value, &child)?,
                            budget,
                            depth + 1,
                            &child,
                            report,
                        )?),
                    );
                }
                out.properties = Some(properties);
            }
            "items" => {
                out.items = Some(Box::new(convert(
                    object(value, &field)?,
                    budget,
                    depth + 1,
                    &field,
                    report,
                )?))
            }
            "anyOf" => {
                let values = value.as_array().filter(|v| !v.is_empty()).ok_or_else(|| {
                    TransformError::shape(&field, "anyOf must be a nonempty array")
                })?;
                let mut variants = Vec::with_capacity(values.len());
                for (index, value) in values.iter().enumerate() {
                    let child = format!("{field}[{index}]");
                    variants.push(Box::new(convert(
                        object(value, &child)?,
                        budget,
                        depth + 1,
                        &child,
                        report,
                    )?));
                }
                out.any_of = Some(variants);
            }
            _ => {
                return Err(TransformError::unsupported(
                    field,
                    "keyword has no exact typed Gemini schema representation",
                ));
            }
        }
    }
    Ok(out)
}

fn schema_type(name: &str, path: &str) -> Result<SchemaType, TransformError> {
    Ok(match name {
        "string" => SchemaType::String,
        "number" => SchemaType::Number,
        "integer" => SchemaType::Integer,
        "boolean" => SchemaType::Boolean,
        "array" => SchemaType::Array,
        "object" => SchemaType::Object,
        "null" => SchemaType::Null,
        _ => {
            return Err(TransformError::shape(
                format!("{path}.type"),
                "unknown JSON Schema type",
            ));
        }
    })
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, TransformError> {
    value
        .as_object()
        .ok_or_else(|| TransformError::unsupported(path, "typed Gemini requires an object schema"))
}

fn decode<T: serde::de::DeserializeOwned>(value: &Value, path: &str) -> Result<T, TransformError> {
    serde_json::from_value(value.clone())
        .map_err(|error| TransformError::shape(path, error.to_string()))
}

fn bound(value: &Value, path: &str) -> Result<String, TransformError> {
    let integer = value.as_i64().filter(|n| *n >= 0).or_else(|| {
        let n = value.as_f64()?;
        (n.is_finite() && (0.0..9_223_372_036_854_775_808.0).contains(&n) && n.fract() == 0.0)
            .then_some(n as i64)
    });
    integer
        .map(|n| n.to_string())
        .ok_or_else(|| TransformError::shape(path, "expected a nonnegative int64 bound"))
}

fn number(value: &Value, path: &str) -> Result<f64, TransformError> {
    let n = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| TransformError::shape(path, "expected a number"))?;
    // Reject integer values whose constraint would change through f64 rounding.
    if value.as_i64().is_some_and(|v| n as i128 != i128::from(v))
        || value.as_u64().is_some_and(|v| n as i128 != i128::from(v))
    {
        return Err(TransformError::unsupported(
            path,
            "numeric bound cannot be represented exactly by Gemini double",
        ));
    }
    Ok(n)
}
