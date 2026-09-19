use super::*;
use crate::{BatchConnectionTrait, D1Type, Projection};

/// A connection whose result decoding follows an explicit projection: the
/// D1 binding and the libSQL pipeline. Discovery runs the same SQLite pragma
/// queries on either.
pub trait ProjectedConnection: BatchConnectionTrait + Sized {
    fn with_projection(&self, projection: Projection) -> Self;
}

impl SchemaSync {
    /// Read schema metadata without mutating it, then produce a reviewable plan.
    pub async fn plan<C: ProjectedConnection>(self, db: &C) -> Result<SyncPlan, DbErr> {
        let snapshot = inspect_schema(
            db,
            &self.tables.keys().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;
        self.plan_snapshot(&snapshot).await
    }
    pub async fn sync<C: ProjectedConnection>(self, db: &C) -> Result<SyncPlan, DbErr> {
        let plan = self.plan(db).await?;
        plan.apply(db).await?;
        Ok(plan)
    }
}

impl SyncPlan {
    /// Execute the generated statements as one atomic batch.
    pub async fn apply<C: BatchConnectionTrait>(&self, db: &C) -> Result<(), DbErr> {
        BatchConnectionTrait::atomic_batch(db, self.statements()).await?;
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl ProjectedConnection for crate::D1Connection {
    fn with_projection(&self, projection: Projection) -> Self {
        crate::D1Connection::with_projection(self, projection)
    }
}

#[cfg(target_arch = "wasm32")]
impl crate::D1Connection {
    pub fn schema_sync(&self) -> SchemaSync {
        SchemaSync::new()
    }

    pub async fn inspect_schema(&self, tables: &[&str]) -> Result<SchemaSnapshot, DbErr> {
        inspect_schema(self, tables).await
    }
}

pub(crate) async fn inspect_schema<C: ProjectedConnection>(
    db: &C,
    tables: &[&str],
) -> Result<SchemaSnapshot, DbErr> {
    let mut snapshot = SchemaSnapshot::default();
    for table in tables {
        if !schema_has_table(db, table).await? {
            continue;
        }
        let mut info = TableInfo::default();
        let projection = Projection::new()
            .column("name", D1Type::Text, false)?
            .column("type", D1Type::Text, false)?
            .column("notnull", D1Type::Bool, false)?
            .column("dflt_value", D1Type::Text, true)?
            .column("pk", D1Type::I64, false)?
            .column("hidden", D1Type::I64, false)?;
        for row in db
            .with_projection(projection)
            .query_all_raw(sql(
                "SELECT name,type,\"notnull\",dflt_value,pk,hidden FROM pragma_table_xinfo(?)",
                vec![(*table).into()],
            ))
            .await?
        {
            let column = ColumnInfo {
                name: row.try_get("", "name")?,
                sql_type: row.try_get("", "type")?,
                not_null: row.try_get("", "notnull")?,
                default_sql: row.try_get("", "dflt_value")?,
                primary_key_position: row.try_get("", "pk")?,
                hidden: row.try_get("", "hidden")?,
            };
            info.columns.insert(column.name.clone(), column);
        }
        let projection = Projection::new()
            .column("name", D1Type::Text, false)?
            .column("unique", D1Type::Bool, false)?
            .column("origin", D1Type::Text, false)?
            .column("partial", D1Type::Bool, false)?;
        for row in db
            .with_projection(projection)
            .query_all_raw(sql(
                "SELECT name,\"unique\",origin,partial FROM pragma_index_list(?)",
                vec![(*table).into()],
            ))
            .await?
        {
            let name: String = row.try_get("", "name")?;
            let columns = db
                .with_projection(Projection::new().column("name", D1Type::Text, true)?)
                .query_all_raw(sql(
                    "SELECT name FROM pragma_index_info(?) ORDER BY seqno",
                    vec![name.clone().into()],
                ))
                .await?
                .into_iter()
                .map(|row| {
                    row.try_get::<Option<String>>("", "name")
                        .map(|name| name.unwrap_or_else(|| "<expression>".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            info.indexes.insert(
                name.clone(),
                IndexInfo {
                    name,
                    columns,
                    unique: row.try_get("", "unique")?,
                    origin: row.try_get("", "origin")?,
                    partial: row.try_get("", "partial")?,
                },
            );
        }
        snapshot.tables.insert((*table).to_owned(), info);
    }
    Ok(snapshot)
}

async fn schema_has_table<C: ProjectedConnection>(db: &C, table: &str) -> Result<bool, DbErr> {
    Ok(db
        .with_projection(Projection::new().column("name", D1Type::Text, false)?)
        .query_one_raw(sql(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name=?",
            vec![table.into()],
        ))
        .await?
        .is_some())
}
