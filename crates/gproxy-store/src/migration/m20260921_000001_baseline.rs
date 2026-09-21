//! The first migration: the schema as the entity registry defines it.

use gproxy_seaorm::sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager};
use sea_orm::{ConnectionTrait, DbErr};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260921_000001_baseline"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Create every registered table, index and foreign key.
    ///
    /// Derived from [`crate::schema`] rather than transcribed, which is the
    /// point: fifty-odd tables written out by hand would be a second definition
    /// of the schema, free to drift from the entities, and nothing but review
    /// would notice. Reading the registry instead makes disagreement impossible
    /// — whatever the entities say today is what a fresh database gets.
    ///
    /// The consequence, and the whole reason the convention in the parent
    /// module exists: this migration is not frozen. It creates *today's* shape,
    /// not 2026-09-21's. That is why every migration appended after it has to be
    /// a no-op when its change is already present — on a fresh database the
    /// baseline has already made it.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        crate::schema(db.get_database_backend()).apply(db).await
    }

    /// There is no way back from the baseline: what precedes it is the empty
    /// database, and dropping fifty tables in an order the registry does not
    /// publish is a guess that fails on a foreign key. An operator who wants
    /// what came before the baseline wants a new database.
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "the baseline cannot be rolled back: what precedes it is an empty database. Point \
             this build at a new one instead."
                .to_owned(),
        ))
    }
}
