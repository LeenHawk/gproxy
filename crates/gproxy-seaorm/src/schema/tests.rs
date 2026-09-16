use super::*;
use futures_executor::block_on;

mod original {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "people")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

mod upgraded {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "people")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        #[sea_orm(renamed_from = "name", unique)]
        pub label: String,
        pub note: Option<String>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

mod bad_upgrade {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "people")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
        pub mandatory: String,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

fn snapshot() -> SchemaSnapshot {
    let columns = [
        ColumnInfo {
            name: "id".into(),
            sql_type: "INTEGER".into(),
            not_null: true,
            default_sql: None,
            primary_key_position: 1,
            hidden: 0,
        },
        ColumnInfo {
            name: "name".into(),
            sql_type: "varchar".into(),
            not_null: true,
            default_sql: None,
            primary_key_position: 0,
            hidden: 0,
        },
    ]
    .into_iter()
    .map(|column| (column.name.clone(), column))
    .collect();
    SchemaSnapshot {
        tables: BTreeMap::from([(
            "people".into(),
            TableInfo {
                columns,
                ..Default::default()
            },
        )]),
    }
}
#[test]
fn unchanged_entity_has_no_ddl() {
    let plan = block_on(
        SchemaSync::new()
            .register(original::Entity)
            .plan_snapshot(&snapshot()),
    )
    .unwrap();
    assert!(plan.is_empty());
}
#[test]
fn creates_missing_table_from_seaorm_ddl() {
    let plan = block_on(
        SchemaSync::new()
            .register(original::Entity)
            .plan_snapshot(&SchemaSnapshot::default()),
    )
    .unwrap();
    assert_eq!(plan.statements().len(), 1);
    assert!(
        plan.statements()[0]
            .sql
            .starts_with("CREATE TABLE \"people\"")
    );
    assert!(
        plan.statements()[0]
            .sql
            .contains("PRIMARY KEY AUTOINCREMENT")
    );
}
#[test]
fn rename_add_column_and_unique_index_are_ordered() {
    let plan = block_on(
        SchemaSync::new()
            .register(upgraded::Entity)
            .plan_snapshot(&snapshot()),
    )
    .unwrap();
    let ddl: Vec<_> = plan.statements().iter().map(|s| s.sql.as_str()).collect();
    assert_eq!(ddl.len(), 3);
    assert_eq!(
        ddl[0],
        "ALTER TABLE \"people\" RENAME COLUMN \"name\" TO \"label\""
    );
    assert!(ddl[1].starts_with("ALTER TABLE \"people\" ADD COLUMN \"note\""));
    assert!(ddl[2].starts_with("CREATE UNIQUE INDEX IF NOT EXISTS \"idx-people-label\""));
}
#[test]
fn add_column_rules_are_left_to_sqlite_and_existing_type_changes_warn() {
    let plan = block_on(
        SchemaSync::new()
            .register(bad_upgrade::Entity)
            .plan_snapshot(&snapshot()),
    )
    .unwrap();
    assert!(plan.statements()[0].sql.contains("mandatory"));
    let mut changed = snapshot();
    changed
        .tables
        .get_mut("people")
        .unwrap()
        .columns
        .get_mut("name")
        .unwrap()
        .sql_type = "INTEGER".into();
    let plan = block_on(
        SchemaSync::new()
            .register(original::Entity)
            .plan_snapshot(&changed),
    )
    .unwrap();
    assert!(plan.is_empty());
    assert_eq!(plan.warnings.len(), 1);
}
#[test]
fn unique_indexes_follow_entity_definitions_without_ownership_rules() {
    let mut existing = snapshot();
    let index = IndexInfo {
        name: "external_unique_name".into(),
        columns: vec!["name".into()],
        unique: true,
        origin: "c".into(),
        partial: false,
    };
    existing
        .tables
        .get_mut("people")
        .unwrap()
        .indexes
        .insert(index.name.clone(), index);
    let plan = block_on(
        SchemaSync::new()
            .register(original::Entity)
            .plan_snapshot(&existing),
    )
    .unwrap();
    assert_eq!(plan.statements().len(), 1);
    assert!(
        plan.statements()[0]
            .sql
            .starts_with("DROP INDEX \"external_unique_name\"")
    );
}
#[test]
fn duplicate_registration_and_unregistered_tables_follow_seaorm() {
    let plan = block_on(
        SchemaSync::new()
            .register(original::Entity)
            .register(original::Entity)
            .plan_snapshot(&SchemaSnapshot::default()),
    )
    .unwrap();
    assert_eq!(plan.statements().len(), 1);
    assert!(
        block_on(SchemaSync::new().plan_snapshot(&snapshot()))
            .unwrap()
            .is_empty()
    );
}
