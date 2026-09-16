use gproxy_protocol::{
    transform::{
        TransformErrorKind,
        generate::gemini_schema::{self, SchemaLimits},
    },
    wire::gemini::content::Schema,
};
use serde_json::json;

#[test]
fn declared_recursive_fields_and_formal_json_stay_separate() {
    let source: Schema = serde_json::from_value(json!({
        "type":"OBJECT", "unknown":{"bad":true}, "required":["value"],
        "properties":{"value":{"type":"STRING", "minLength":"2", "pattern":"^a", "unknown":123}},
        "default":{"rest":{"keep":true}}, "example":null, "propertyOrdering":["value"]
    }))
    .unwrap();
    let converted = gemini_schema::to_json(&source, SchemaLimits::default()).unwrap();
    assert_eq!(converted.value, json!({"type":"object", "required":["value"], "properties":{"value":{"type":"string","minLength":2,"pattern":"^a"}}, "default":{"rest":{"keep":true}}, "examples":[null]}).as_object().unwrap().clone());
    assert_eq!(converted.report.diagnostics.len(), 1);
    let restored = gemini_schema::from_json(&converted.value, SchemaLimits::default())
        .unwrap()
        .value;
    assert!(restored.rest.is_empty());
    assert!(
        restored.properties.as_ref().unwrap()["value"]
            .rest
            .is_empty()
    );
    assert_eq!(restored.example, Some(serde_json::Value::Null));
    assert_eq!(restored.default, Some(json!({"rest":{"keep":true}})));
}

#[test]
fn nullable_expands_type_without_weakening_other_constraints() {
    let source: Schema = serde_json::from_value(json!({"type":"STRING", "nullable":true, "enum":["yes"], "anyOf":[{"type":"STRING","pattern":"^y"}]})).unwrap();
    let result = gemini_schema::to_json(&source, SchemaLimits::default()).unwrap();
    assert_eq!(result.value["type"], json!(["string", "null"]));
    assert_eq!(result.value["enum"], json!(["yes"]));
    assert_eq!(
        result.value["anyOf"],
        json!([{"type":"string","pattern":"^y"}])
    );
    let restored = gemini_schema::from_json(&result.value, SchemaLimits::default()).unwrap();
    assert_eq!(restored.value, source);
}

#[test]
fn necessary_constraints_cannot_be_silently_weakened() {
    for keyword in [
        "oneOf",
        "additionalProperties",
        "$ref",
        "not",
        "uniqueItems",
    ] {
        let source = json!({"type":"object",keyword:false});
        assert!(
            gemini_schema::from_json(source.as_object().unwrap(), SchemaLimits::default()).is_ok()
        );
    }
    let source = json!({"type":"number","minimum":9007199254740993_u64});
    assert_eq!(
        gemini_schema::from_json(source.as_object().unwrap(), SchemaLimits::default())
            .unwrap_err()
            .kind(),
        TransformErrorKind::Unsupported
    );
}

#[test]
fn int64_bounds_and_conversion_budgets_are_enforced() {
    for bound in ["-1", "9223372036854775808", "1.5"] {
        let source: Schema =
            serde_json::from_value(json!({"type":"ARRAY","maxItems":bound})).unwrap();
        assert_eq!(
            gemini_schema::to_json(&source, SchemaLimits::default())
                .unwrap_err()
                .kind(),
            TransformErrorKind::InvalidInput
        );
    }
    let source: Schema =
        serde_json::from_value(json!({"type":"ARRAY","items":{"type":"STRING"}})).unwrap();
    for limits in [
        SchemaLimits {
            max_depth: 0,
            max_nodes: 10,
        },
        SchemaLimits {
            max_depth: 10,
            max_nodes: 1,
        },
    ] {
        assert_eq!(
            gemini_schema::to_json(&source, limits).unwrap_err().kind(),
            TransformErrorKind::Limit
        );
        let value = json!({"type":"array","items":{"type":"string"}});
        assert_eq!(
            gemini_schema::from_json(value.as_object().unwrap(), limits)
                .unwrap_err()
                .kind(),
            TransformErrorKind::Limit
        );
    }
}

#[test]
fn empty_constraint_arrays_fail_and_integral_json_floats_map_exactly() {
    for source in [
        json!({"type":"STRING","enum":[]}),
        json!({"type":"OBJECT","anyOf":[]}),
    ] {
        let source: Schema = serde_json::from_value(source).unwrap();
        assert!(gemini_schema::to_json(&source, SchemaLimits::default()).is_err());
    }
    for source in [
        json!({"type":"string","enum":[]}),
        json!({"type":"object","anyOf":[]}),
    ] {
        assert!(
            gemini_schema::from_json(source.as_object().unwrap(), SchemaLimits::default()).is_err()
        );
    }
    let source = json!({"type":"array","minItems":2.0,"maxItems":9223372036854775807_i64});
    let converted = gemini_schema::from_json(source.as_object().unwrap(), SchemaLimits::default())
        .unwrap()
        .value;
    assert_eq!(converted.min_items.as_deref(), Some("2"));
    assert_eq!(converted.max_items.as_deref(), Some("9223372036854775807"));
    for bound in [json!(2.5), json!(9223372036854775808_u64), json!(-1.0)] {
        let source = json!({"type":"array","minItems":bound});
        assert!(
            gemini_schema::from_json(source.as_object().unwrap(), SchemaLimits::default()).is_err()
        );
    }
}
