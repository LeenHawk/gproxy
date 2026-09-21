use sea_orm::{ConnectionTrait, DbBackend, DbErr, EntityTrait, SchemaBuilder, Statement};
use sea_orm_migration::{MigrationStatus, MigratorTrait};

/// Shared entity registration for SeaORM's native builder and the D1 adapter.
pub trait EntityRegistry: Sized + Send {
    fn register<E: EntityTrait>(self, entity: E) -> Self;
}

impl EntityRegistry for SchemaBuilder {
    fn register<E: EntityTrait>(self, entity: E) -> Self {
        SchemaBuilder::register(self, entity)
    }
}

impl EntityRegistry for super::SchemaSync {
    fn register<E: EntityTrait>(self, entity: E) -> Self {
        super::SchemaSync::register(self, entity)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncReport {
    /// D1 planner warnings about existing column types left unchanged. Native
    /// SeaORM emits its own diagnostics through logging instead of this list.
    pub warnings: Vec<String>,
}

/// The tables an engine keeps for itself, which no census of "what is in this
/// database" should count. `sqlite_*` is SQLite's own catalogue; `_cf_*` is what
/// D1 puts beside a binding's data.
pub fn is_engine_table(name: &str) -> bool {
    name.starts_with("sqlite_") || name.starts_with("_cf_")
}

/// Entity-first initialization, the table census the schema gate reads, and the
/// official migration runner. Drivers and schema discovery stay here; the caller
/// supplies its entity registry and its migrator.
///
/// There is deliberately no method that walks the registry against a live schema
/// emitting `ALTER`s. That existed, it was the only thing standing between this
/// workspace and a versioned migration, and it died halfway through the first
/// column it could not add. Creating a schema and changing one are different
/// operations with different failure modes, and only the second one needs a
/// ledger: [`apply_schema`](Self::apply_schema) does the first,
/// [`run_migrations`](Self::run_migrations) does the second.
#[async_trait::async_trait]
pub trait SchemaSyncConnectionTrait: ConnectionTrait {
    type Registry: EntityRegistry;

    fn schema_registry(&self) -> Self::Registry;

    /// Create every registered table, index and foreign key, as on an empty
    /// database. Errors if any of them already exists — this is the fresh
    /// install, not a repair.
    async fn apply_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr>;

    /// The tables this database already holds, engine-owned ones excluded and
    /// order unspecified. One cheap, portable fact, which is all the question
    /// "may this build open this database" is decided from.
    async fn table_names(&self) -> Result<Vec<String>, DbErr>;

    /// Apply pending migrations with SeaORM's official runner.
    ///
    /// The default refuses: not every connection this adapter offers can be
    /// handed to the runner. Overridden for native SeaORM connections and for
    /// the D1 binding, which reaches the runner through its own proxy bridge.
    async fn run_migrations<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        let _ = steps;
        Err(no_runner())
    }

    /// What the ledger says about each migration this build carries, in the
    /// order the migrator declares them.
    async fn migration_report<M: MigratorTrait>(
        &self,
    ) -> Result<Vec<(String, MigrationStatus)>, DbErr> {
        Err(no_runner())
    }
}

/// The name of the table a migrator keeps its ledger in — `seaql_migrations`
/// unless the migrator overrides it. The one place that answers "what would this
/// build's own bookkeeping be called", so a schema gate never has to guess.
pub fn ledger_table_name<M: MigratorTrait>() -> String {
    M::migration_table_name().to_string()
}

fn no_runner() -> DbErr {
    crate::error(
        "this connection cannot drive SeaORM's migration runner; migrate the database with a \
         native connection to it, or with `gproxy migrate`",
    )
}

/// The migrator's declared order, each entry marked against a ledger read
/// elsewhere. Used by the connections that read their own ledger rather than
/// letting the runner do it.
#[cfg(any(target_arch = "wasm32", feature = "libsql"))]
fn status_against<M: MigratorTrait>(applied: &[String]) -> Vec<(String, MigrationStatus)> {
    M::migrations()
        .into_iter()
        .map(|migration| {
            let name = migration.name().to_owned();
            let status = if applied.iter().any(|version| version == &name) {
                MigrationStatus::Applied
            } else {
                MigrationStatus::Pending
            };
            (name, status)
        })
        .collect()
}

/// Native SeaORM connections.
///
/// The `IntoSchemaManagerConnection` bound is what lets this impl hand `self` to
/// the official runner, which takes a connection it can open a transaction on
/// rather than any `ConnectionTrait`. `DatabaseConnection` and
/// `DatabaseTransaction` satisfy it; nothing else in this workspace is used as a
/// native store connection.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl<C> SchemaSyncConnectionTrait for C
where
    C: ConnectionTrait + sea_schema::Connection,
    for<'c> &'c C: sea_orm_migration::IntoSchemaManagerConnection<'c>,
{
    type Registry = SchemaBuilder;

    fn schema_registry(&self) -> Self::Registry {
        SchemaBuilder::new(sea_orm::Schema::new(self.get_database_backend()))
    }

    async fn apply_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        registry.apply(self).await?;
        Ok(SyncReport::default())
    }

    async fn table_names(&self) -> Result<Vec<String>, DbErr> {
        // Per-backend because there is no portable catalogue, and aliased to
        // `name` so the one row reader below serves all three.
        let sql = match self.get_database_backend() {
            DbBackend::Sqlite => "SELECT name FROM sqlite_schema WHERE type = 'table'",
            DbBackend::Postgres => {
                "SELECT tablename::text AS name FROM pg_catalog.pg_tables \
                 WHERE schemaname = current_schema()"
            }
            DbBackend::MySql => {
                "SELECT CAST(table_name AS CHAR) AS name FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE'"
            }
            // `DatabaseBackend` is `non_exhaustive`. A backend this build has
            // never heard of is not one whose catalogue it can guess at.
            other => {
                return Err(DbErr::BackendNotSupported {
                    db: other.as_str(),
                    ctx: "table_names",
                });
            }
        };
        let mut names = Vec::new();
        // Disambiguated: `sea_schema::Connection` carries a `query_all_raw` of
        // its own, and this one wants SeaORM's.
        let rows = ConnectionTrait::query_all_raw(
            self,
            Statement::from_string(self.get_database_backend(), sql),
        )
        .await?;
        for row in rows {
            let name: String = row.try_get("", "name")?;
            if !is_engine_table(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }

    async fn run_migrations<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        M::up(self, steps).await
    }

    async fn migration_report<M: MigratorTrait>(
        &self,
    ) -> Result<Vec<(String, MigrationStatus)>, DbErr> {
        Ok(M::get_migration_with_status(self)
            .await?
            .into_iter()
            .map(|migration| (migration.name().to_owned(), migration.status()))
            .collect())
    }
}

#[cfg(feature = "libsql")]
#[async_trait::async_trait]
impl SchemaSyncConnectionTrait for crate::LibsqlConnection {
    type Registry = super::SchemaSync;

    fn schema_registry(&self) -> Self::Registry {
        super::SchemaSync::new()
    }

    async fn apply_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        Ok(SyncReport {
            warnings: super::apply_registry(registry, self).await?,
        })
    }

    async fn table_names(&self) -> Result<Vec<String>, DbErr> {
        super::table_names(self).await
    }

    /// The ledger is readable here even though the runner is not reachable, so
    /// a libSQL host can still tell whether the database it was handed is the
    /// one this build expects — and refuse on its own terms if it is behind.
    async fn migration_report<M: MigratorTrait>(
        &self,
    ) -> Result<Vec<(String, MigrationStatus)>, DbErr> {
        let applied = super::ledger_versions(self, &ledger_table_name::<M>()).await?;
        Ok(status_against::<M>(&applied))
    }

    // `run_migrations` keeps the refusing default: the runner needs a
    // `DatabaseConnection`, and the proxy bridge that gives D1 one is built on
    // SeaORM's `proxy` feature, which this crate can only enable on wasm32 — it
    // does not compile next to `sqlx-sqlite`.
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait]
impl SchemaSyncConnectionTrait for crate::D1Connection {
    type Registry = super::SchemaSync;

    fn schema_registry(&self) -> Self::Registry {
        self.schema_sync()
    }

    async fn apply_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        Ok(SyncReport {
            warnings: super::apply_registry(registry, self).await?,
        })
    }

    async fn table_names(&self) -> Result<Vec<String>, DbErr> {
        super::table_names(self).await
    }

    async fn run_migrations<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        self.migrate_up::<M>(steps).await
    }

    async fn migration_report<M: MigratorTrait>(
        &self,
    ) -> Result<Vec<(String, MigrationStatus)>, DbErr> {
        crate::D1Connection::migration_status::<M>(self).await
    }
}
