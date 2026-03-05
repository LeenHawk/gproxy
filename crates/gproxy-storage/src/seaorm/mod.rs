pub mod entities;
mod store_mutation;
mod store_query;
mod write_sink;

use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, DbErr};

#[derive(Clone)]
pub struct SeaOrmStorage {
    db: DatabaseConnection,
}

impl SeaOrmStorage {
    pub async fn connect(dsn: &str) -> Result<Self, DbErr> {
        let db = Database::connect(dsn).await?;
        if db.get_database_backend() == DatabaseBackend::Sqlite {
            db.execute_unprepared("PRAGMA foreign_keys = ON").await?;
        }
        Ok(Self { db })
    }

    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }

    /// SeaORM 2.0 entity-first schema sync.
    pub async fn sync(&self) -> Result<(), DbErr> {
        let schema = self
            .db
            .get_schema_registry("gproxy_storage::seaorm::entities::*");
        schema.sync(&self.db).await
    }
}
