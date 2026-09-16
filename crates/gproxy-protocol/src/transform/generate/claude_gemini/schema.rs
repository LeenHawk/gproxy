use crate::{Rest, transform::TransformError, wire::claude::tools as ct};
use serde_json::Value;

pub(super) fn to_json(input: &ct::JsonSchema) -> Result<Rest, TransformError> {
    let mut output = Rest::new();
    output.insert("type".into(), Value::String("object".into()));
    if let Some(value) = &input.properties {
        output.insert("properties".into(), value.clone());
    }
    if let Some(value) = &input.required {
        output.insert("required".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.additional_properties {
        output.insert("additionalProperties".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.defs {
        output.insert("$defs".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.definitions {
        output.insert("definitions".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.schema_uri {
        output.insert("$schema".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.id {
        output.insert("$id".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.reference {
        output.insert("$ref".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.anchor {
        output.insert("$anchor".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.dynamic_reference {
        output.insert("$dynamicRef".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.dynamic_anchor {
        output.insert("$dynamicAnchor".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.comment {
        output.insert("$comment".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.title {
        output.insert("title".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.description {
        output.insert("description".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.default_value {
        output.insert("default".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.examples {
        output.insert("examples".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.enum_values {
        output.insert("enum".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.const_value {
        output.insert("const".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.all_of {
        output.insert("allOf".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.any_of {
        output.insert("anyOf".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.one_of {
        output.insert("oneOf".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.not {
        output.insert("not".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.if_schema {
        output.insert("if".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.then_schema {
        output.insert("then".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.else_schema {
        output.insert("else".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.pattern_properties {
        output.insert("patternProperties".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.property_names {
        output.insert("propertyNames".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.dependent_required {
        output.insert("dependentRequired".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.dependent_schemas {
        output.insert("dependentSchemas".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.dependencies {
        output.insert("dependencies".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.unevaluated_properties {
        output.insert("unevaluatedProperties".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.min_properties {
        output.insert("minProperties".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.max_properties {
        output.insert("maxProperties".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.read_only {
        output.insert("readOnly".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.write_only {
        output.insert("writeOnly".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = &input.deprecated {
        output.insert("deprecated".into(), serde_json::to_value(value)?);
    }
    Ok(output)
}

pub(super) fn from_json(input: &Rest) -> Result<ct::JsonSchema, TransformError> {
    if input.get("type").is_some_and(|value| value != "object") {
        return Err(TransformError::unsupported(
            "tools.parameters.type",
            "Claude tools require object schemas",
        ));
    }
    let mut output = ct::JsonSchema::builder(ct::JsonSchemaType::Object).build();
    for (key, value) in input {
        match key.as_str() {
            "type" => {}
            "properties" => {
                if !value.is_object() {
                    return Err(TransformError::shape(
                        "tools.parameters.properties",
                        "properties must be an object",
                    ));
                }
                output.properties = Some(value.clone());
            }
            "required" => output.required = decode(value, key).ok(),
            "additionalProperties" => output.additional_properties = decode(value, key).ok(),
            "$defs" => output.defs = decode(value, key).ok(),
            "definitions" => output.definitions = decode(value, key).ok(),
            "$schema" => output.schema_uri = decode(value, key).ok(),
            "$id" => output.id = decode(value, key).ok(),
            "$ref" => output.reference = decode(value, key).ok(),
            "$anchor" => output.anchor = decode(value, key).ok(),
            "$dynamicRef" => output.dynamic_reference = decode(value, key).ok(),
            "$dynamicAnchor" => output.dynamic_anchor = decode(value, key).ok(),
            "$comment" => output.comment = decode(value, key).ok(),
            "title" => output.title = decode(value, key).ok(),
            "description" => output.description = decode(value, key).ok(),
            "default" => output.default_value = decode(value, key).ok(),
            "examples" => output.examples = decode(value, key).ok(),
            "enum" => output.enum_values = decode(value, key).ok(),
            "const" => output.const_value = decode(value, key).ok(),
            "allOf" => output.all_of = decode(value, key).ok(),
            "anyOf" => output.any_of = decode(value, key).ok(),
            "oneOf" => output.one_of = decode(value, key).ok(),
            "not" => output.not = decode(value, key).ok(),
            "if" => output.if_schema = decode(value, key).ok(),
            "then" => output.then_schema = decode(value, key).ok(),
            "else" => output.else_schema = decode(value, key).ok(),
            "patternProperties" => output.pattern_properties = decode(value, key).ok(),
            "propertyNames" => output.property_names = decode(value, key).ok(),
            "dependentRequired" => output.dependent_required = decode(value, key).ok(),
            "dependentSchemas" => output.dependent_schemas = decode(value, key).ok(),
            "dependencies" => output.dependencies = decode(value, key).ok(),
            "unevaluatedProperties" => output.unevaluated_properties = decode(value, key).ok(),
            "minProperties" => output.min_properties = decode(value, key).ok(),
            "maxProperties" => output.max_properties = decode(value, key).ok(),
            "readOnly" => output.read_only = decode(value, key).ok(),
            "writeOnly" => output.write_only = decode(value, key).ok(),
            "deprecated" => output.deprecated = decode(value, key).ok(),
            _ => {
                continue;
            }
        }
    }
    Ok(output)
}
fn decode<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Result<T, TransformError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        TransformError::shape(format!("tools.parameters.{key}"), error.to_string())
    })
}
