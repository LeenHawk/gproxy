use sea_orm::{DbBackend, Schema, Statement};

pub mod parent {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_parents")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        #[sea_orm(column_name = "display_name")]
        pub name: String,
        pub status: Status,
        #[sea_orm(has_many)]
        pub children: HasMany<super::child::Entity>,
        #[sea_orm(has_one)]
        pub profile: HasOne<super::profile::Entity>,
    }
    impl ActiveModelBehavior for ActiveModel {}
    #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "projection_status")]
    pub enum Status {
        #[sea_orm(string_value = "ready")]
        Ready,
    }
}

pub mod child {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_children")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub parent_id: i32,
        pub name: String,
        pub note: Option<String>,
        #[sea_orm(belongs_to, from = "parent_id", to = "id")]
        pub parent: BelongsTo<super::parent::Entity>,
        #[sea_orm(has_many, via = "child_tag")]
        pub tags: HasMany<super::tag::Entity>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod profile {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_profiles")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        #[sea_orm(unique)]
        pub parent_id: i32,
        pub label: String,
        #[sea_orm(belongs_to, from = "parent_id", to = "id")]
        pub parent: BelongsTo<super::parent::Entity>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod tag {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_tags")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub label: String,
        #[sea_orm(has_many, via = "child_tag")]
        pub children: HasMany<super::child::Entity>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod child_tag {
    use sea_orm::entity::prelude::*;
    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projection_child_tags")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub child_id: i32,
        #[sea_orm(primary_key, auto_increment = false)]
        pub tag_id: i32,
        #[sea_orm(belongs_to, from = "child_id", to = "id")]
        pub child: BelongsTo<super::child::Entity>,
        #[sea_orm(belongs_to, from = "tag_id", to = "id")]
        pub tag: BelongsTo<super::tag::Entity>,
    }
    impl ActiveModelBehavior for ActiveModel {}
}

pub fn schema_statements(backend: DbBackend) -> Vec<Statement> {
    let schema = Schema::new(backend);
    [
        schema.create_table_from_entity(parent::Entity),
        schema.create_table_from_entity(child::Entity),
        schema.create_table_from_entity(profile::Entity),
        schema.create_table_from_entity(tag::Entity),
        schema.create_table_from_entity(child_tag::Entity),
    ]
    .iter()
    .map(|table| backend.build(table))
    .collect()
}

pub const SEED: &[&str] = &[
    "INSERT INTO projection_parents VALUES (1,'one','ready'),(2,'empty','ready')",
    "INSERT INTO projection_children VALUES (10,1,'first',NULL),(11,1,'second','note')",
    "INSERT INTO projection_profiles VALUES (100,1,'profile')",
    "INSERT INTO projection_tags VALUES (20,'rust'),(21,'sql')",
    "INSERT INTO projection_child_tags VALUES (10,20),(10,21)",
];
