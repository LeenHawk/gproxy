use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement};
use sea_orm_migration::SchemaManager;

/// D1 equivalents of SchemaManager's SQLx-gated inspection helpers.
#[async_trait::async_trait]
pub trait D1SchemaManagerExt {
    async fn d1_has_table(&self, table: &str) -> Result<bool, DbErr>;
    async fn d1_has_column(&self, table: &str, column: &str) -> Result<bool, DbErr>;
    async fn d1_has_index(&self, table: &str, index: &str) -> Result<bool, DbErr>;
}

#[async_trait::async_trait]
impl D1SchemaManagerExt for SchemaManager<'_> {
    async fn d1_has_table(&self, table: &str) -> Result<bool, DbErr> {
        exists(self.get_connection(), "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?) AS __d1_exists", vec![table.into()]).await
    }
    async fn d1_has_column(&self, table: &str, column: &str) -> Result<bool, DbErr> {
        exists(
            self.get_connection(),
            "SELECT EXISTS(SELECT 1 FROM pragma_table_xinfo(?) WHERE name=?) AS __d1_exists",
            vec![table.into(), column.into()],
        )
        .await
    }
    async fn d1_has_index(&self, table: &str, index: &str) -> Result<bool, DbErr> {
        exists(
            self.get_connection(),
            "SELECT EXISTS(SELECT 1 FROM pragma_index_list(?) WHERE name=?) AS __d1_exists",
            vec![table.into(), index.into()],
        )
        .await
    }
}

async fn exists<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<bool, DbErr> {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await?
    .ok_or_else(|| crate::error("missing D1 schema existence result"))?
    .try_get("", "__d1_exists")
}
