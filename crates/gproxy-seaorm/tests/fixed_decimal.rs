#![cfg(not(target_arch = "wasm32"))]
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal, FixedDecimalError, SelectProjection};
use rust_decimal::Decimal;
use sea_orm::FromQueryResult;
use sea_orm::{
    ActiveValue::Set, ConnectOptions, Database, DbBackend, QueryOrder, QueryTrait, Schema,
    entity::prelude::*,
};

mod amount {
    use super::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "amounts")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i32,
        #[sea_orm(
            column_type = "BigInteger",
            select_as = "char(32)",
            save_as = "decimal(20,0)"
        )]
        pub value: FixedDecimal,
        #[sea_orm(
            column_type = "BigInteger",
            select_as = "char(32)",
            save_as = "decimal(20,0)"
        )]
        pub optional: Option<FixedDecimal>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}
#[test]
fn exact_range_and_rounding_are_explicit() {
    let x: FixedDecimal = "9007199.254740993".parse().unwrap();
    assert_eq!(x.atoms(), 9_007_199_254_740_993);
    for n in [i64::MIN, i64::MAX, 0, 1, -1] {
        let value = FixedDecimal::from_atoms(n);
        assert_eq!(value.to_string().parse::<FixedDecimal>().unwrap(), value);
        assert_eq!(
            serde_json::from_str::<FixedDecimal>(&serde_json::to_string(&value).unwrap()).unwrap(),
            value
        );
    }
    assert_eq!(
        "0.0000000001".parse::<FixedDecimal>(),
        Err(FixedDecimalError::Precision)
    );
    assert_eq!(
        "9223372036.854775808".parse::<FixedDecimal>(),
        Err(FixedDecimalError::Overflow)
    );
    assert!(serde_json::from_str::<FixedDecimal>("0.1").is_err());
    // Decimal's ordinary parser may round a long tail; exact input must reject it.
    assert!(
        "1.00000000000000000000000000001"
            .parse::<FixedDecimal>()
            .is_err()
    );
    assert_eq!(
        FixedDecimal::rounded("0.0000000005".parse::<Decimal>().unwrap())
            .unwrap()
            .atoms(),
        0
    );
    assert_eq!(
        FixedDecimal::rounded("0.0000000015".parse::<Decimal>().unwrap())
            .unwrap()
            .atoms(),
        2
    );
    assert!(
        FixedDecimal::from_atoms(i64::MAX)
            .checked_add(FixedDecimal::from_atoms(1))
            .is_err()
    );
    assert_eq!(
        "0.1"
            .parse::<FixedDecimal>()
            .unwrap()
            .checked_add("0.2".parse().unwrap())
            .unwrap()
            .to_string(),
        "0.3"
    );
}
#[tokio::test]
async fn sql_integer_storage_text_transport_and_numeric_filters() {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    db.execute(&Schema::new(DbBackend::Sqlite).create_table_from_entity(amount::Entity))
        .await
        .unwrap();
    let values = [
        FixedDecimal::from_atoms(i64::MIN),
        "0.1".parse().unwrap(),
        "9007199.254740993".parse().unwrap(),
        FixedDecimal::from_atoms(i64::MAX),
    ];
    let stmts = values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            amount::Entity::insert(amount::ActiveModel {
                id: Set(i as i32),
                value: Set(*value),
                optional: Set(None),
            })
            .build(DbBackend::Sqlite)
        })
        .collect::<Vec<_>>();
    db.atomic_batch(&stmts).await.unwrap();
    let select = amount::Entity::find().order_by_asc(amount::Column::Value);
    let batches = db
        .query_batch(&[select.batch_query(DbBackend::Sqlite).unwrap()])
        .await
        .unwrap();
    let models = batches[0]
        .iter()
        .map(|r| amount::Model::from_query_result(r, "").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(models.iter().map(|m| m.value).collect::<Vec<_>>(), values);
    assert!(models.iter().all(|m| m.optional.is_none()));
    let positive = amount::Entity::find()
        .filter(amount::Column::Value.gt("1".parse::<FixedDecimal>().unwrap()))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(positive.len(), 2);
    let types = db
        .query_all_raw(sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "SELECT typeof(value) AS kind FROM amounts",
        ))
        .await
        .unwrap();
    assert!(
        types
            .iter()
            .all(|r| r.try_get::<String>("", "kind").unwrap() == "integer")
    );
    for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
        let statement = amount::Entity::insert(amount::ActiveModel {
            id: Set(4),
            value: Set(values[3]),
            optional: Set(Some(values[0])),
        })
        .build(backend);
        assert!(statement.sql.contains("decimal(20,0)"), "{}", statement.sql);
        assert!(
            amount::Entity::find()
                .build(backend)
                .sql
                .contains("char(32)")
        );
    }
}
