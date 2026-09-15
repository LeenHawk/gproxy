use sea_orm::{FromQueryResult, ModelTrait, QueryResult, Value};
use serde_json::json;

use crate::{D1Type, Projection, codec};

mod sample {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "different_business_table")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub flag: bool,
        pub payload: Vec<u8>,
        pub json_data: Json,
        pub optional_counter: Option<i64>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

#[test]
fn entity_metadata_decodes_types_without_business_column_names() {
    let projection = Projection::for_entity::<sample::Entity>().unwrap();
    let rows = codec::rows(
        &json!([
            ["json_data", "payload", "flag", "id", "optional_counter"],
            ["{\"中文\":[1,null]}", [0, 128, 255], 1, 23, null]
        ]),
        &projection,
    )
    .unwrap();
    let row: QueryResult = rows.into_iter().next().unwrap().into();
    let model = sample::Model::from_query_result(&row, "").unwrap();
    assert_eq!(model.id, 23);
    assert!(model.flag);
    assert_eq!(model.payload, [0, 128, 255]);
    assert_eq!(model.json_data, json!({"中文": [1,null]}));
    assert_eq!(model.optional_counter, None);
    assert_eq!(model.get(sample::Column::Id), Value::Int(Some(23)));
}

#[test]
fn invalid_optional_value_is_an_error_not_a_silent_null() {
    let projection = Projection::for_entity::<sample::Entity>().unwrap();
    assert!(
        codec::rows(
            &json!([["optional_counter"], ["not an integer"]]),
            &projection
        )
        .is_err()
    );
}

#[test]
fn rejects_unknown_duplicate_missing_and_non_nullable_result_columns() {
    let projection = Projection::new().column("x", D1Type::I64, false).unwrap();
    for raw in [
        json!([["unknown"], [1]]),
        json!([["x", "x"], [1, 2]]),
        json!([["x"], []]),
        json!([["x"], [null]]),
        json!([]),
    ] {
        assert!(codec::rows(&raw, &projection).is_err(), "{raw}");
    }
    assert!(projection.clone().column("x", D1Type::Text, true).is_err());
    assert!(Projection::new().column("", D1Type::Text, true).is_err());
    assert!(
        codec::rows(&json!([["x"]]), &projection)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn positional_projection_preserves_sql_order_not_sorted_aliases() {
    let projection = Projection::new()
        .column("z", D1Type::Text, false)
        .unwrap()
        .column("a", D1Type::I32, false)
        .unwrap()
        .by_index();
    let rows = codec::rows(&json!([["z", "a"], ["first", 99]]), &projection).unwrap();
    let row: QueryResult = rows.into_iter().next().unwrap().into();
    assert_eq!(row.try_get_by_index::<String>(0).unwrap(), "first");
    assert_eq!(row.try_get_by_index::<i32>(1).unwrap(), 99);
}

#[test]
fn numeric_boundaries_and_explicit_exact_text() {
    let projection = Projection::new().column("x", D1Type::I64, false).unwrap();
    for value in [9_007_199_254_740_991_i64, -9_007_199_254_740_991] {
        assert!(codec::parameter(&Value::BigInt(Some(value))).is_ok());
        assert!(codec::rows(&json!([["x"], [value]]), &projection).is_ok());
    }
    for value in [
        9_007_199_254_740_992_i64,
        -9_007_199_254_740_992,
        i64::MAX,
        i64::MIN,
    ] {
        assert!(codec::parameter(&Value::BigInt(Some(value))).is_err());
        assert!(codec::rows(&json!([["x"], [value]]), &projection).is_err());
        let rows = codec::rows(&json!([["x"], [value.to_string()]]), &projection).unwrap();
        assert_eq!(rows[0].values["x"], Value::BigInt(Some(value)));
    }
    assert!(codec::parameter(&Value::BigUnsigned(Some(u64::MAX))).is_err());
    for value in [json!(1.5), json!("9223372036854775808")] {
        assert!(codec::rows(&json!([["x"], [value]]), &projection).is_err());
    }
}

#[test]
fn parameters_keep_sql_types_and_blob_bytes() {
    assert_eq!(
        codec::parameter(&Value::BigUnsigned(Some(1))).unwrap(),
        codec::Parameter::Number(1.0)
    );
    assert_eq!(
        codec::parameter(&Value::Bool(Some(false))).unwrap(),
        codec::Parameter::Number(0.0)
    );
    assert_eq!(
        codec::parameter(&Value::String(Some("42".into()))).unwrap(),
        codec::Parameter::Text("42".into())
    );
    assert_eq!(
        codec::parameter(&Value::Bytes(Some(vec![0, 128, 255]))).unwrap(),
        codec::Parameter::Bytes(vec![0, 128, 255])
    );
    assert_eq!(
        codec::parameter(&Value::Bytes(Some(vec![]))).unwrap(),
        codec::Parameter::Bytes(vec![])
    );
    assert_eq!(
        codec::parameter(&Value::BigInt(None)).unwrap(),
        codec::Parameter::Null
    );
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert!(codec::parameter(&Value::Double(Some(value))).is_err());
    }
}

#[test]
fn rejects_invalid_bool_blob_json_and_integer_width() {
    for (kind, value) in [
        (D1Type::Bool, json!(2)),
        (D1Type::Bytes, json!([256])),
        (D1Type::Bytes, json!("X'00'")),
        (D1Type::Json, json!("{bad")),
        (D1Type::I8, json!(128)),
        (D1Type::U32, json!(-1)),
        (D1Type::F32, json!(1e100)),
    ] {
        let projection = Projection::new().column("x", kind, false).unwrap();
        assert!(codec::rows(&json!([["x"], [value]]), &projection).is_err());
    }
}

#[test]
fn metadata_uses_logical_changes_and_rejects_invalid_acknowledgements() {
    let result = codec::execution(
        &json!({"success":true,"meta":{"changes":1,"rows_written":4,"last_row_id":23}}),
    )
    .unwrap();
    assert_eq!(result.rows_affected, 1);
    assert_eq!(result.last_insert_id, 23);
    for raw in [
        json!({"success":false}),
        json!({"success":true,"meta":{"rows_written":1}}),
        json!({"success":true,"meta":{"changes":-1}}),
        json!({"success":true,"meta":{"changes":1,"last_row_id":9007199254740992_i64}}),
    ] {
        assert!(codec::execution(&raw).is_err());
    }
}
