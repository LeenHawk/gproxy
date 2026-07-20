use crate::store::libsql::LibsqlClient;
use crate::store::persistence::traits::CorePersistence;

use super::{LibsqlPersistence, schema};

impl LibsqlPersistence {
    /// Create a new persistence backend and ensure the schema exists.
    pub async fn connect(url: String, token: String) -> anyhow::Result<Self> {
        let client = LibsqlClient::new(url, token);
        schema::ensure_schema(&client).await?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait(?Send)]
impl CorePersistence for LibsqlPersistence {
    fn kind(&self) -> &'static str {
        "libsql"
    }

    async fn health(&self) -> anyhow::Result<()> {
        self.client
            .execute("SELECT 1", &[])
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("libsql health failed: {e}"))
    }
}
