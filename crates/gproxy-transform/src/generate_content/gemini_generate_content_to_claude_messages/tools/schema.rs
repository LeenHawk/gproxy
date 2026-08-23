use gproxy_protocol::claude;

use crate::TransformError;

pub(super) fn convert(mut value: serde_json::Value) -> Result<claude::JsonSchema, TransformError> {
    normalize(&mut value);
    let serde_json::Value::Object(mut object) = value else {
        return Err(TransformError::shape(
            "Gemini tool schema",
            "expected an object",
        ));
    };
    object.remove("type");
    let properties = take_object(&mut object, "properties")?;
    let required = object
        .remove("required")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_else(Vec::new);
    Ok(claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties,
        required,
        rest: object,
    })
}

pub(super) fn empty() -> claude::JsonSchema {
    claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties: Default::default(),
        required: Vec::new(),
        rest: Default::default(),
    }
}

fn take_object(
    map: &mut claude::JsonObject,
    key: &str,
) -> Result<claude::JsonObject, TransformError> {
    map.remove(key)
        .map(serde_json::from_value)
        .transpose()
        .map(|value| value.unwrap_or_else(Default::default))
        .map_err(Into::into)
}

fn normalize(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(kind) = map.get_mut("type") {
                normalize_type(kind);
            }
            map.values_mut().for_each(normalize);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(normalize),
        _ => {}
    }
}

fn normalize_type(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(kind) => kind.make_ascii_lowercase(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(normalize_type),
        _ => {}
    }
}
