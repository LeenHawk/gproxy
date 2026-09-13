use crate::{Rest, transform::TransformError, wire::claude::tools as ct};
use serde_json::Value;

pub(super) fn to_chat(input: &ct::JsonSchema) -> Result<Rest, TransformError> {
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

pub(super) fn from_chat(input: &Rest) -> Result<ct::JsonSchema, TransformError> {
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
            "required" => output.required = Some(decode(value, key)?),
            "additionalProperties" => output.additional_properties = Some(decode(value, key)?),
            "$defs" => output.defs = Some(decode(value, key)?),
            "definitions" => output.definitions = Some(decode(value, key)?),
            "$schema" => output.schema_uri = Some(decode(value, key)?),
            "$id" => output.id = Some(decode(value, key)?),
            "$ref" => output.reference = Some(decode(value, key)?),
            "$anchor" => output.anchor = Some(decode(value, key)?),
            "$dynamicRef" => output.dynamic_reference = Some(decode(value, key)?),
            "$dynamicAnchor" => output.dynamic_anchor = Some(decode(value, key)?),
            "$comment" => output.comment = Some(decode(value, key)?),
            "title" => output.title = Some(decode(value, key)?),
            "description" => output.description = Some(decode(value, key)?),
            "default" => output.default_value = Some(decode(value, key)?),
            "examples" => output.examples = Some(decode(value, key)?),
            "enum" => output.enum_values = Some(decode(value, key)?),
            "const" => output.const_value = Some(decode(value, key)?),
            "allOf" => output.all_of = Some(decode(value, key)?),
            "anyOf" => output.any_of = Some(decode(value, key)?),
            "oneOf" => output.one_of = Some(decode(value, key)?),
            "not" => output.not = Some(decode(value, key)?),
            "if" => output.if_schema = Some(decode(value, key)?),
            "then" => output.then_schema = Some(decode(value, key)?),
            "else" => output.else_schema = Some(decode(value, key)?),
            "patternProperties" => output.pattern_properties = Some(decode(value, key)?),
            "propertyNames" => output.property_names = Some(decode(value, key)?),
            "dependentRequired" => output.dependent_required = Some(decode(value, key)?),
            "dependentSchemas" => output.dependent_schemas = Some(decode(value, key)?),
            "dependencies" => output.dependencies = Some(decode(value, key)?),
            "unevaluatedProperties" => output.unevaluated_properties = Some(decode(value, key)?),
            "minProperties" => output.min_properties = Some(decode(value, key)?),
            "maxProperties" => output.max_properties = Some(decode(value, key)?),
            "readOnly" => output.read_only = Some(decode(value, key)?),
            "writeOnly" => output.write_only = Some(decode(value, key)?),
            "deprecated" => output.deprecated = Some(decode(value, key)?),
            _ => {
                return Err(TransformError::unsupported(
                    format!("tools.parameters.{key}"),
                    "schema keyword needs a declared Claude wire field; it cannot be tunneled through Rest",
                ));
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
