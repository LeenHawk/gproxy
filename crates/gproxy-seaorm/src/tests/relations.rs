use crate::{D1Type, Projection, SelectProjection};
use sea_orm::{ColumnTrait, DbBackend, EntityTrait, QueryOrder, QuerySelect, QueryTrait};

#[path = "../../tests/support/relations.rs"]
// Database setup is also reused by the external WASM/D1 integration harness.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
mod entities;
use entities::{child, parent, profile, tag};

#[test]
fn relation_aliases_and_optional_entities_are_derived() {
    let direct = parent::Entity::find().projection().unwrap();
    assert_eq!(
        direct.column_type("display_name"),
        Some((D1Type::Text, false))
    );
    assert_eq!(direct.column_type("status"), Some((D1Type::Text, false)));
    let left = parent::Entity::find()
        .find_also_related(child::Entity)
        .projection()
        .unwrap();
    assert_eq!(left.column_type("A_id"), Some((D1Type::I32, false)));
    assert_eq!(left.column_type("B_id"), Some((D1Type::I32, true)));
    assert_eq!(left.column_type("B_note"), Some((D1Type::Text, true)));
    let inner = parent::Entity::find()
        .find_both_related(child::Entity)
        .projection()
        .unwrap();
    assert_eq!(inner.column_type("B_id"), Some((D1Type::I32, false)));
    let three = parent::Entity::find()
        .find_also_related(child::Entity)
        .find_also_related(profile::Entity)
        .projection()
        .unwrap();
    assert_eq!(three.column_type("C_id"), Some((D1Type::I32, true)));
    assert_eq!(three.column_type("C_label"), Some((D1Type::Text, true)));
    let prefixed = Projection::for_entity_prefixed::<child::Entity>("joined_", true).unwrap();
    assert_eq!(
        prefixed.column_type("joined_parent_id"),
        Some((D1Type::I32, true))
    );
    let many = child::Entity::find()
        .find_with_related(tag::Entity)
        .projection()
        .unwrap();
    assert_eq!(many.column_type("B_label"), Some((D1Type::Text, true)));
}

#[test]
fn selecting_columns_does_not_mutate_the_query_or_guess_computed_types() {
    let query = parent::Entity::find()
        .select_only()
        .column(parent::Column::Id)
        .order_by_asc(parent::Column::Id)
        .limit(3);
    let before = query.build(DbBackend::Sqlite);
    let batch = query.batch_query(DbBackend::Sqlite).unwrap();
    assert_eq!(before.sql, batch.statement.sql);
    assert_eq!(
        batch.projection.column_type("id"),
        Some((D1Type::I32, false))
    );
    assert_eq!(batch.projection.column_type("display_name"), None);
    assert!(
        parent::Entity::find()
            .select_only()
            .column_as(parent::Column::Id.count(), "id")
            .batch_query(DbBackend::Sqlite)
            .is_err()
    );
    assert!(
        parent::Entity::find()
            .select_only()
            .column_as(parent::Column::Id, "custom_id")
            .projection()
            .is_err()
    );
    assert!(
        parent::Entity::find()
            .select_only()
            .column(parent::Column::Id)
            .column(parent::Column::Id)
            .projection()
            .is_err()
    );
    assert!(
        parent::Entity::find()
            .select_only()
            .column_as(profile::Column::Id, "id")
            .projection()
            .is_err()
    );
}

mod priced {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_priced")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub amount: Decimal,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

#[test]
fn only_selected_columns_require_supported_d1_types() {
    assert!(priced::Entity::find().projection().is_err());
    let projection = priced::Entity::find()
        .select_only()
        .column(priced::Column::Id)
        .projection()
        .unwrap();
    assert_eq!(projection.column_type("id"), Some((D1Type::I32, false)));
    assert_eq!(projection.column_type("amount"), None);
}

#[cfg(not(target_arch = "wasm32"))]
async fn database() -> sea_orm::DatabaseConnection {
    use sea_orm::{ConnectOptions, ConnectionTrait, Database, Statement};
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    for statement in entities::schema_statements(DbBackend::Sqlite) {
        db.execute_raw(statement).await.unwrap();
    }
    for sql in entities::SEED {
        db.execute_raw(Statement::from_string(DbBackend::Sqlite, *sql))
            .await
            .unwrap();
    }
    db
}

// Execute actual ORM-generated SQL on SQLite, then feed its named result objects
// through the D1 batch decoder and SeaORM's own model/optional-model selectors.
#[cfg(not(target_arch = "wasm32"))]
async fn d1_rows<Q: SelectProjection>(
    db: &sea_orm::DatabaseConnection,
    query: &Q,
) -> Vec<sea_orm::QueryResult> {
    use crate::BatchConnectionTrait;
    use serde_json::{Map, Value, json};
    let batch = query.batch_query(DbBackend::Sqlite).unwrap();
    let rows = db
        .query_batch(std::slice::from_ref(&batch))
        .await
        .unwrap()
        .remove(0);
    let objects = rows
        .iter()
        .map(|row| {
            batch
                .projection
                .columns
                .iter()
                .map(|(name, column)| {
                    let value = match column.kind {
                        D1Type::I32 => json!(row.try_get::<Option<i32>>("", name).unwrap()),
                        D1Type::Text => json!(row.try_get::<Option<String>>("", name).unwrap()),
                        other => panic!("unexpected fixture type {other:?}"),
                    };
                    (name.clone(), value)
                })
                .collect::<Map<_, _>>()
        })
        .map(Value::Object)
        .collect::<Vec<_>>();
    crate::codec::batch_rows(
        &json!({"success":true,"results":objects}),
        &batch.projection,
    )
    .unwrap()
    .into_iter()
    .map(super::query_result)
    .collect()
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn real_left_inner_and_three_way_joins_roundtrip_through_d1_decoder() {
    use sea_orm::{SelectThreeModel, SelectTwoModel, SelectTwoRequiredModel, SelectorTrait};
    let db = database().await;
    let left = parent::Entity::find()
        .find_also_related(child::Entity)
        .order_by_asc(parent::Column::Id)
        .order_by_asc(child::Column::Id);
    let expected = left.clone().all(&db).await.unwrap();
    let actual = d1_rows(&db, &left)
        .await
        .into_iter()
        .map(SelectTwoModel::<parent::Model, child::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
    assert!(actual.last().unwrap().1.is_none());
    let inner = parent::Entity::find()
        .find_both_related(child::Entity)
        .order_by_asc(child::Column::Id);
    let expected = inner.clone().all(&db).await.unwrap();
    let actual = d1_rows(&db, &inner)
        .await
        .into_iter()
        .map(SelectTwoRequiredModel::<parent::Model, child::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actual, expected);
    let three = parent::Entity::find()
        .find_also_related(child::Entity)
        .find_also_related(profile::Entity)
        .order_by_asc(parent::Column::Id)
        .order_by_asc(child::Column::Id);
    let expected = three.clone().all(&db).await.unwrap();
    let actual = d1_rows(&db, &three)
        .await
        .into_iter()
        .map(SelectThreeModel::<parent::Model, child::Model, profile::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actual, expected);
    assert!(actual.last().unwrap().2.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn many_to_many_and_one_to_one_keep_missing_relations_optional() {
    use sea_orm::{SelectTwoModel, SelectorTrait};
    let db = database().await;
    let query = child::Entity::find()
        .find_with_related(tag::Entity)
        .order_by_asc(child::Column::Id)
        .order_by_asc(tag::Column::Id);
    let rows = d1_rows(&db, &query)
        .await
        .into_iter()
        .map(SelectTwoModel::<child::Model, tag::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .filter_map(|r| r.1.as_ref().map(|t| t.id))
            .collect::<Vec<_>>(),
        [20, 21]
    );
    assert_eq!(rows.last().unwrap().0.id, 11);
    assert!(rows.last().unwrap().1.is_none());
    let one = parent::Entity::find()
        .find_also_related(profile::Entity)
        .order_by_asc(parent::Column::Id);
    let expected = one.clone().all(&db).await.unwrap();
    let actual = d1_rows(&db, &one)
        .await
        .into_iter()
        .map(SelectTwoModel::<parent::Model, profile::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn bound_conditions_relation_filters_and_exists_survive_projection() {
    use sea_orm::sea_query::{Expr, ExprTrait, Query};
    use sea_orm::{Condition, QueryFilter, SelectTwoModel, SelectorTrait};
    let db = database().await;
    let query = parent::Entity::find()
        .find_also_related(child::Entity)
        .filter(
            Condition::all()
                .add(parent::Column::Id.is_in([1, 2]))
                .add(parent::Column::Id.gte(1))
                .add(
                    Condition::any()
                        .add(parent::Column::Name.contains("one"))
                        .add(parent::Column::Id.eq(2)),
                )
                .add(child::Column::Note.is_null()),
        )
        .order_by_asc(parent::Column::Id);
    let rows = d1_rows(&db, &query)
        .await
        .into_iter()
        .map(SelectTwoModel::<parent::Model, child::Model>::from_raw_query_result)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.iter().map(|row| row.0.id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(rows[0].1.as_ref().unwrap().id, 10);
    assert!(rows[1].1.is_none());

    let needle = "one' OR 1=1 --";
    let quoted = parent::Entity::find().filter(parent::Column::Name.contains(needle));
    assert!(
        !quoted
            .batch_query(DbBackend::Sqlite)
            .unwrap()
            .statement
            .sql
            .contains(needle)
    );
    assert!(d1_rows(&db, &quoted).await.is_empty());

    let exists = Query::select()
        .expr(Expr::val(1))
        .from(child::Entity)
        .and_where(
            Expr::col((child::Entity, child::Column::ParentId))
                .equals((parent::Entity, parent::Column::Id)),
        )
        .and_where(child::Column::Name.eq("first"))
        .to_owned();
    let parents = parent::Entity::find().filter(Expr::exists(exists));
    let rows = d1_rows(&db, &parents).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].try_get::<i32>("", "id").unwrap(), 1);
}
