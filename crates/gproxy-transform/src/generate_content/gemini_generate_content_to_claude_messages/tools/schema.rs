use gproxy_protocol::claude;

use crate::TransformError;

pub(super) fn typed_value(
    schema: gproxy_protocol::gemini::Schema,
) -> Result<serde_json::Value, TransformError> {
    let mut object = serde_json::Map::new();
    object.insert("type".into(), schema_type(schema.r#type)?.into());
    insert(&mut object, "format", schema.format)?;
    insert(&mut object, "title", schema.title)?;
    insert(&mut object, "description", schema.description)?;
    insert(&mut object, "nullable", schema.nullable)?;
    insert(&mut object, "enum", schema.r#enum)?;
    if let Some(properties) = schema.properties {
        let properties = properties
            .into_iter()
            .map(|(name, schema)| typed_value(schema).map(|schema| (name, schema)))
            .collect::<Result<serde_json::Map<_, _>, _>>()?;
        object.insert("properties".into(), properties.into());
    }
    insert(&mut object, "required", schema.required)?;
    if let Some(any_of) = schema.any_of {
        let any_of = any_of
            .into_iter()
            .map(typed_value)
            .collect::<Result<Vec<_>, _>>()?;
        object.insert("anyOf".into(), any_of.into());
    }
    insert(&mut object, "propertyOrdering", schema.property_ordering)?;
    if let Some(items) = schema.items {
        object.insert("items".into(), typed_value(*items)?);
    }
    insert(&mut object, "minimum", schema.minimum)?;
    insert(&mut object, "maximum", schema.maximum)?;
    insert(&mut object, "maxItems", schema.max_items)?;
    insert(&mut object, "minItems", schema.min_items)?;
    insert(&mut object, "minProperties", schema.min_properties)?;
    insert(&mut object, "maxProperties", schema.max_properties)?;
    insert(&mut object, "minLength", schema.min_length)?;
    insert(&mut object, "maxLength", schema.max_length)?;
    insert(&mut object, "pattern", schema.pattern)?;
    insert(&mut object, "example", schema.example)?;
    insert(&mut object, "default", schema.default)?;
    Ok(object.into())
}

fn schema_type(kind: gproxy_protocol::gemini::SchemaType) -> Result<&'static str, TransformError> {
    use gproxy_protocol::gemini::{SchemaType, SchemaTypeKnown};

    match kind {
        SchemaType::Known(SchemaTypeKnown::String) => Ok("string"),
        SchemaType::Known(SchemaTypeKnown::Number) => Ok("number"),
        SchemaType::Known(SchemaTypeKnown::Integer) => Ok("integer"),
        SchemaType::Known(SchemaTypeKnown::Boolean) => Ok("boolean"),
        SchemaType::Known(SchemaTypeKnown::Array) => Ok("array"),
        SchemaType::Known(SchemaTypeKnown::Object) => Ok("object"),
        SchemaType::Known(SchemaTypeKnown::Null) => Ok("null"),
        SchemaType::Known(SchemaTypeKnown::TypeUnspecified) => Err(TransformError::unsupported(
            "Gemini schema type",
            "unspecified type",
        )),
        SchemaType::Unknown(value) => Err(TransformError::unsupported("Gemini schema type", value)),
        _ => Err(TransformError::unsupported(
            "Gemini schema type",
            "future type",
        )),
    }
}

fn insert<T: serde::Serialize>(
    object: &mut claude::JsonObject,
    key: &str,
    value: Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        object.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}

pub(super) fn convert(mut value: serde_json::Value) -> Result<claude::JsonSchema, TransformError> {
    crate::common::gemini_schema::normalize(&mut value);
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
        rest: Default::default(),
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
