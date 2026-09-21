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
    .try_get("", EXISTS_COLUMN)
}

/// The alias every probe below selects its answer under. It is the D1
/// projection's name because D1 is the only connection that needs the column
/// declared in advance; the other backends simply inherit it.
const EXISTS_COLUMN: &str = "__d1_exists";

/// Ask a live schema what it already has, on whichever backend the migration is
/// running against.
///
/// This exists because a migration in this workspace has to be a no-op when its
/// change is already present — the baseline reads the entity registry, so a
/// fresh database is created with today's shape and every later migration meets
/// its own work already done. See `gproxy_store::migration` for the convention.
///
/// `SchemaManager` has `has_table`/`has_column`/`has_index` of its own, and they
/// are not what a migration here should call: each is compiled per backend
/// behind `sea-orm-migration`'s `sqlx-*` features, which this workspace enables
/// on `sea-orm` but not on `sea-orm-migration`, so they answer
/// `BackendNotSupported` rather than the question. These probe SQL directly, and
/// the names are deliberately distinct — an inherent method would win the lookup
/// against a trait one and the difference would be invisible at the call site.
#[async_trait::async_trait]
pub trait SchemaProbeExt {
    async fn probe_table(&self, table: &str) -> Result<bool, DbErr>;
    async fn probe_column(&self, table: &str, column: &str) -> Result<bool, DbErr>;
    async fn probe_index(&self, table: &str, index: &str) -> Result<bool, DbErr>;
}

#[async_trait::async_trait]
impl SchemaProbeExt for SchemaManager<'_> {
    async fn probe_table(&self, table: &str) -> Result<bool, DbErr> {
        match self.get_database_backend() {
            DbBackend::Sqlite => self.d1_has_table(table).await,
            backend => {
                probe(
                    self.get_connection(),
                    backend,
                    match backend {
                        DbBackend::Postgres => {
                            "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
                             WHERE table_schema = current_schema() AND table_name = $1) \
                             AS __d1_exists"
                        }
                        _ => {
                            "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
                             WHERE table_schema = DATABASE() AND table_name = ?) AS __d1_exists"
                        }
                    },
                    vec![table.into()],
                )
                .await
            }
        }
    }

    async fn probe_column(&self, table: &str, column: &str) -> Result<bool, DbErr> {
        match self.get_database_backend() {
            DbBackend::Sqlite => self.d1_has_column(table, column).await,
            backend => {
                probe(
                    self.get_connection(),
                    backend,
                    match backend {
                        DbBackend::Postgres => {
                            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
                             WHERE table_schema = current_schema() AND table_name = $1 \
                             AND column_name = $2) AS __d1_exists"
                        }
                        _ => {
                            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
                             WHERE table_schema = DATABASE() AND table_name = ? \
                             AND column_name = ?) AS __d1_exists"
                        }
                    },
                    vec![table.into(), column.into()],
                )
                .await
            }
        }
    }

    async fn probe_index(&self, table: &str, index: &str) -> Result<bool, DbErr> {
        match self.get_database_backend() {
            DbBackend::Sqlite => self.d1_has_index(table, index).await,
            backend => {
                probe(
                    self.get_connection(),
                    backend,
                    match backend {
                        DbBackend::Postgres => {
                            "SELECT EXISTS(SELECT 1 FROM pg_indexes \
                             WHERE schemaname = current_schema() AND tablename = $1 \
                             AND indexname = $2) AS __d1_exists"
                        }
                        _ => {
                            "SELECT EXISTS(SELECT 1 FROM information_schema.statistics \
                             WHERE table_schema = DATABASE() AND table_name = ? \
                             AND index_name = ?) AS __d1_exists"
                        }
                    },
                    vec![table.into(), index.into()],
                )
                .await
            }
        }
    }
}

/// The non-SQLite half of the probes. `EXISTS` comes back as a boolean on
/// Postgres and as an integer on MySQL, so the answer is read at whichever type
/// the backend actually produced rather than at one this crate would prefer.
async fn probe<C: ConnectionTrait>(
    db: &C,
    backend: DbBackend,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<bool, DbErr> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(backend, sql, values))
        .await?
        .ok_or_else(|| crate::error("missing schema existence result"))?;
    match backend {
        DbBackend::Postgres => row.try_get("", EXISTS_COLUMN),
        _ => Ok(row.try_get::<i64>("", EXISTS_COLUMN)? != 0),
    }
}
