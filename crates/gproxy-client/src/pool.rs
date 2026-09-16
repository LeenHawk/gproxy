use std::{sync::Arc, time::Duration};

use moka::future::Cache;

use crate::{Client, ConnectionConfig, Error};

/// A bounded cache of clients, each owning its backend's connection pool.
/// Concurrent misses for the same parameters coalesce into one construction.
/// Capacity and idle eviction drop cache ownership, never cancel in-flight work.
#[derive(Clone)]
pub struct ClientPool {
    cache: Cache<ClientKey, Arc<Client>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ClientKey {
    config: ConnectionConfig,
    http1_only: bool,
}

impl Default for ClientPool {
    fn default() -> Self {
        Self::new(256, Duration::from_secs(600)).expect("valid default cache limits")
    }
}

impl ClientPool {
    /// These limits concern cached clients, independently of idle sockets per host.
    /// Eviction is maintained by Moka on access or by explicit prune calls.
    pub fn new(max_clients: u64, idle_timeout: Duration) -> Result<Self, Error> {
        if max_clients == 0 || idle_timeout.is_zero() {
            return Err(Error::InvalidConfig(
                "client cache capacity and idle timeout must be positive",
            ));
        }
        Ok(Self {
            cache: Cache::builder()
                .max_capacity(max_clients)
                .time_to_idle(idle_timeout)
                .build(),
        })
    }

    /// Requires a Tokio runtime. Configuration IDs/names never participate in lookup.
    /// Build errors are shared with concurrent waiters and are not cached.
    pub async fn get(&self, config: &ConnectionConfig) -> Result<Arc<Client>, Arc<Error>> {
        self.get_with_protocol(config, false).await
    }

    /// Obtain a client for RFC 6455 WS/WSS upgrades. Force HTTP/1.1, including
    /// ALPN, so a server supporting HTTP/2 cannot silently negotiate an incompatible
    /// handshake. Proxy, TLS emulation and other profile settings are preserved.
    /// The HTTP/1.1 override participates in caching independently of regular HTTP.
    pub async fn get_websocket(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<Client>, Arc<Error>> {
        self.get_with_protocol(config, true).await
    }

    async fn get_with_protocol(
        &self,
        config: &ConnectionConfig,
        http1_only: bool,
    ) -> Result<Arc<Client>, Arc<Error>> {
        let config = config.clone().normalized().map_err(Arc::new)?;
        let key = ClientKey { config, http1_only };
        self.cache
            .try_get_with(key.clone(), async move {
                tokio::task::spawn_blocking(move || {
                    Client::build(&key.config, key.http1_only).map(Arc::new)
                })
                .await
                .map_err(Error::BuildTask)?
            })
            .await
    }

    /// Remove cached handles; outstanding clients/responses remain usable.
    /// Call after host system-proxy changes before obtaining further clients.
    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Run pending expiry/capacity maintenance. Hosts may call this periodically
    /// so an otherwise idle cache also releases its references promptly.
    pub async fn prune(&self) {
        self.cache.run_pending_tasks().await;
    }
}
