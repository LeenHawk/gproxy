mod admin;
mod bindings;
mod control;
mod credentials;
mod identity;
mod recent_usage;
mod runtime;
mod snapshot;
mod tokenizers;
mod usage;

pub use runtime::CleanupResult;

use crate::backend::{self, BackendConfig, Executor, SharedExecutor, Statement};
use crate::schema::Dialect;
use crate::{StoreError, migration};

#[derive(Clone)]
pub struct Store {
    pub(crate) executor: SharedExecutor,
}

impl Store {
    pub async fn open(config: BackendConfig) -> Result<Self, StoreError> {
        let dialect = match &config {
            #[cfg(not(target_arch = "wasm32"))]
            BackendConfig::Sqlite { .. } => Dialect::NativeSqlite,
            BackendConfig::Libsql { .. } => Dialect::Libsql,
        };
        let executor = backend::open(config).await?;
        migration::migrate(executor.as_ref(), dialect).await?;
        Ok(Self { executor })
    }

    pub(crate) fn backend(&self) -> &dyn Executor {
        self.executor.as_ref()
    }

    async fn insert(&self, statement: Statement) -> Result<i64, StoreError> {
        self.backend()
            .execute(statement)
            .await?
            .last_insert_id
            .ok_or_else(|| StoreError::Database("insert did not return a row id".into()))
    }

    async fn update(&self, statement: Statement) -> Result<bool, StoreError> {
        Ok(self.backend().execute(statement).await?.affected_rows == 1)
    }

    async fn delete(&self, statement: Statement) -> Result<bool, StoreError> {
        Ok(self.backend().execute(statement).await?.affected_rows == 1)
    }
}
