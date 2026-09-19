use sea_orm::{ConnectionTrait, DbErr, EntityTrait, SchemaBuilder};

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

/// Entity-first initialization and incremental synchronization. Drivers and
/// schema discovery stay here; the caller supplies its entity registry.
#[async_trait::async_trait]
pub trait SchemaSyncConnectionTrait: ConnectionTrait {
    type Registry: EntityRegistry;

    fn schema_registry(&self) -> Self::Registry;
    async fn sync_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr>;
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl<C: ConnectionTrait + sea_schema::Connection> SchemaSyncConnectionTrait for C {
    type Registry = SchemaBuilder;

    fn schema_registry(&self) -> Self::Registry {
        SchemaBuilder::new(sea_orm::Schema::new(self.get_database_backend()))
    }

    async fn sync_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        registry.sync(self).await?;
        Ok(SyncReport::default())
    }
}

#[cfg(feature = "libsql")]
#[async_trait::async_trait]
impl SchemaSyncConnectionTrait for crate::LibsqlConnection {
    type Registry = super::SchemaSync;

    fn schema_registry(&self) -> Self::Registry {
        super::SchemaSync::new()
    }

    async fn sync_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        let plan = registry.sync(self).await?;
        Ok(SyncReport {
            warnings: plan.warnings,
        })
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait]
impl SchemaSyncConnectionTrait for crate::D1Connection {
    type Registry = super::SchemaSync;

    fn schema_registry(&self) -> Self::Registry {
        self.schema_sync()
    }

    async fn sync_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        let plan = registry.sync(self).await?;
        Ok(SyncReport {
            warnings: plan.warnings,
        })
    }
}
