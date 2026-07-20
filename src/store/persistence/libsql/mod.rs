//! Edge (wasm32) persistence backend backed by libSQL/Turso over Hrana HTTP.

mod authz;
mod batch;
mod identity;
mod logs;
mod metrics;
mod ops;
mod pricing;
mod provider;
mod routing;
mod row;
mod schema;
mod settings;
mod setup;
mod tokenize;
mod transform;
mod usage;
mod util;

use crate::store::libsql::LibsqlClient;

/// Edge persistence backend backed by a Turso/libSQL database via Hrana HTTP.
pub struct LibsqlPersistence {
    client: LibsqlClient,
}

/// Map a libsql/Hrana error from an insert/update: a SQLite UNIQUE-constraint
/// violation becomes a persistence conflict; anything else passes through.
pub(crate) fn conflict_if_unique(
    e: crate::store::libsql::StoreError,
    msg: impl FnOnce() -> String,
) -> anyhow::Error {
    if e.to_string().contains("UNIQUE constraint failed") {
        crate::store::persistence::ConflictError::new(msg()).into()
    } else {
        anyhow::anyhow!("libsql: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::PersistenceBackend;

    #[wasm_bindgen_test::wasm_bindgen_test]
    #[ignore = "requires live Turso creds via GPROXY_TEST_TURSO_URL / GPROXY_TEST_TURSO_TOKEN"]
    async fn integration_health() {
        let url = std::env::var("GPROXY_TEST_TURSO_URL").expect("GPROXY_TEST_TURSO_URL");
        let token = std::env::var("GPROXY_TEST_TURSO_TOKEN").expect("GPROXY_TEST_TURSO_TOKEN");
        let backend = LibsqlPersistence::connect(url, token)
            .await
            .expect("connect");
        backend.health().await.expect("health");
    }
}
