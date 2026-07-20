//! `PersistenceBackend` capability implementations for [`DbPersistence`].

mod authz;
mod identity;
mod provider;
mod routing;
mod settings;
mod usage;

use async_trait::async_trait;

use super::DbPersistence;
use crate::store::persistence::traits::CorePersistence;

#[async_trait]
impl CorePersistence for DbPersistence {
    fn kind(&self) -> &'static str {
        "db"
    }

    async fn health(&self) -> anyhow::Result<()> {
        self.conn
            .ping()
            .await
            .map_err(|e| anyhow::anyhow!("db ping failed: {e}"))
    }
}
