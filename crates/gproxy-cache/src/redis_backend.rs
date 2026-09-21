use crate::*;
use futures_util::StreamExt;
use redis::{
    Script,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use std::sync::{Arc, LazyLock};
use tokio::{sync::broadcast, task::JoinHandle};

static VALUES: LazyLock<Script> = LazyLock::new(|| Script::new(include_str!("redis/values.lua")));
static COUNTERS: LazyLock<Script> =
    LazyLock::new(|| Script::new(include_str!("redis/counters.lua")));
static PERMITS: LazyLock<Script> = LazyLock::new(|| Script::new(include_str!("redis/permits.lua")));

#[derive(Clone, Debug)]
pub struct RedisOptions {
    /// Distinct deployments must use distinct namespaces. Never use raw secrets.
    pub namespace: String,
    pub connection_timeout: Duration,
    pub response_timeout: Duration,
    pub limits: Limits,
}
impl RedisOptions {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            connection_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(5),
            limits: Limits::default(),
        }
    }
}
/// Native Redis/Valkey client. Clones share a multiplexed connection manager;
/// independently connected clients with the same namespace share Redis state.
#[derive(Clone)]
pub struct RedisCache {
    client: redis::Client,
    manager: ConnectionManager,
    prefix: String,
    options: Arc<RedisOptions>,
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}
fn version(bytes: Vec<u8>) -> Result<Version> {
    Ok(Version(bytes.try_into().map_err(|_| CacheError::Corrupt)?))
}
fn number(value: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(CacheError::Corrupt)
}
fn counter_result(value: String, token: Vec<u8>) -> Result<Counter> {
    Ok(Counter {
        value: number(&value)?,
        generation: version(token)?,
    })
}
impl RedisCache {
    pub async fn connect(url: &str, options: RedisOptions) -> Result<Self> {
        options.limits.validate()?;
        options.limits.key(&options.namespace)?;
        if options.connection_timeout.is_zero() || options.response_timeout.is_zero() {
            return Err(CacheError::Invalid("Redis timeouts must be positive"));
        }
        let client = redis::Client::open(url)?;
        let config = ConnectionManagerConfig::new()
            .set_number_of_retries(0)
            .set_connection_timeout(Some(options.connection_timeout))
            .set_response_timeout(Some(options.response_timeout));
        let manager = tokio::time::timeout(
            options.connection_timeout,
            ConnectionManager::new_with_config(client.clone(), config),
        )
        .await
        .map_err(|_| CacheError::Timeout)??;
        Ok(Self {
            // Redis Pub/Sub is not isolated by SELECT database. Include the DB
            // number explicitly so notifications follow the same scope as data.
            prefix: format!(
                "gproxy-cache:v1:db{}:{}",
                client.get_connection_info().redis_settings().db(),
                hex(options.namespace.as_bytes())
            ),
            client,
            manager,
            options: Arc::new(options),
        })
    }
    fn key(&self, domain: &str, key: &str) -> Result<String> {
        self.options.limits.key(key)?;
        Ok(format!("{}:{domain}:{}", self.prefix, hex(key.as_bytes())))
    }
    async fn permit(
        &self,
        op: &str,
        key: &str,
        owner: Version,
        ttl: u64,
        limit: u32,
    ) -> Result<bool> {
        let result: i64 = PERMITS
            .key(self.key("permit", key)?)
            .arg(op)
            .arg(owner.as_bytes().as_slice())
            .arg(ttl)
            .arg(limit)
            .invoke_async(&mut self.manager.clone())
            .await?;
        match result {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CacheError::Corrupt),
        }
    }
}
#[async_trait::async_trait]
impl Cache for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<Entry>> {
        let (status, fields): (i64, Vec<Vec<u8>>) = VALUES
            .key(self.key("value", key)?)
            .arg("get")
            .invoke_async(&mut self.manager.clone())
            .await?;
        match (status, fields.as_slice()) {
            (0, []) => Ok(None),
            (1, [token, value]) => Ok(Some(Entry {
                value: value.clone(),
                version: version(token.clone())?,
            })),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn put(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<Version> {
        self.options.limits.value(&value)?;
        let token = Version::fresh()?;
        let (status, _): (i64, Vec<Vec<u8>>) = VALUES
            .key(self.key("value", key)?)
            .arg("put")
            .arg("")
            .arg(token.as_bytes().as_slice())
            .arg(value)
            .arg(ttl_ms(ttl)?)
            .invoke_async(&mut self.manager.clone())
            .await?;
        if status != 1 {
            return Err(CacheError::Corrupt);
        }
        Ok(token)
    }
    async fn delete(&self, key: &str) -> Result<bool> {
        let (status, _): (i64, Vec<Vec<u8>>) = VALUES
            .key(self.key("value", key)?)
            .arg("delete")
            .invoke_async(&mut self.manager.clone())
            .await?;
        match status {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<Replacement>,
    ) -> Result<CasOutcome> {
        let (token, value, ttl) = match replacement {
            Some(replacement) => {
                self.options.limits.value(&replacement.value)?;
                (
                    Some(Version::fresh()?),
                    replacement.value,
                    ttl_ms(replacement.ttl)?,
                )
            }
            None => (None, Vec::new(), 0),
        };
        let (status, _): (i64, Vec<Vec<u8>>) = VALUES
            .key(self.key("value", key)?)
            .arg("cas")
            .arg(
                expected
                    .as_ref()
                    .map_or(&[][..], |v| v.as_bytes().as_slice()),
            )
            .arg(token.as_ref().map_or(&[][..], |v| v.as_bytes().as_slice()))
            .arg(value)
            .arg(ttl)
            .invoke_async(&mut self.manager.clone())
            .await?;
        match status {
            0 => Ok(CasOutcome::Conflict),
            1 => Ok(CasOutcome::Applied(token)),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn counter(&self, key: &str) -> Result<Option<Counter>> {
        let (status, value, token): (i64, String, Vec<u8>) = COUNTERS
            .key(self.key("counter", key)?)
            .arg("get")
            .invoke_async(&mut self.manager.clone())
            .await?;
        match status {
            0 => Ok(None),
            1 => Ok(Some(counter_result(value, token)?)),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn increment(
        &self,
        key: &str,
        amount: u64,
        limit: u64,
        ttl: Duration,
    ) -> Result<IncrementOutcome> {
        counter_args(amount, limit)?;
        let threshold = limit
            .checked_sub(amount)
            .map_or_else(|| "-1".to_owned(), |n| n.to_string());
        let token = Version::fresh()?;
        let (status, value, generation): (i64, String, Vec<u8>) = COUNTERS
            .key(self.key("counter", key)?)
            .arg("increment")
            .arg(amount.to_string())
            .arg(threshold)
            .arg(token.as_bytes().as_slice())
            .arg(ttl_ms(ttl)?)
            .invoke_async(&mut self.manager.clone())
            .await?;
        match status {
            1 => Ok(IncrementOutcome::Applied(counter_result(
                value, generation,
            )?)),
            2 => Ok(IncrementOutcome::Limited {
                current: number(&value)?,
            }),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn decrement(
        &self,
        key: &str,
        generation: Version,
        amount: u64,
    ) -> Result<Option<Counter>> {
        counter_args(amount, i64::MAX as u64)?;
        let (status, value, token): (i64, String, Vec<u8>) = COUNTERS
            .key(self.key("counter", key)?)
            .arg("decrement")
            .arg(generation.as_bytes().as_slice())
            .arg(amount.to_string())
            .invoke_async(&mut self.manager.clone())
            .await?;
        match status {
            0 => Ok(None),
            1 => Ok(Some(counter_result(value, token)?)),
            -1 => Err(CacheError::Underflow),
            _ => Err(CacheError::Corrupt),
        }
    }
    async fn acquire_permit(
        &self,
        key: &str,
        limit: u32,
        ttl: Duration,
    ) -> Result<Option<Version>> {
        self.options.limits.permits(limit)?;
        let owner = Version::fresh()?;
        Ok(self
            .permit("acquire", key, owner, ttl_ms(ttl)?, limit)
            .await?
            .then_some(owner))
    }
    async fn renew_permit(&self, key: &str, owner: Version, ttl: Duration) -> Result<bool> {
        self.permit("renew", key, owner, ttl_ms(ttl)?, 0).await
    }
    async fn release_permit(&self, key: &str, owner: Version) -> Result<bool> {
        self.permit("release", key, owner, 0, 0).await
    }
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<()> {
        self.options.limits.value(&payload)?;
        let _: i64 = redis::cmd("PUBLISH")
            .arg(self.key("notify", topic)?)
            .arg(payload)
            .query_async(&mut self.manager.clone())
            .await?;
        Ok(())
    }
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Subscription>> {
        let key = self.key("notify", topic)?;
        let pubsub = tokio::time::timeout(self.options.connection_timeout, async {
            let mut pubsub = self.client.get_async_pubsub().await?;
            pubsub.subscribe(key).await?;
            Ok::<_, redis::RedisError>(pubsub)
        })
        .await
        .map_err(|_| CacheError::Timeout)??;
        let (sender, receiver) = broadcast::channel(self.options.limits.notification_capacity);
        let max = self.options.limits.max_value_bytes;
        let mut stream = pubsub.into_on_message();
        let task = tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                let bytes = message.get_payload_bytes();
                let event = if bytes.len() > max {
                    Notification::ResyncRequired
                } else {
                    Notification::Message(bytes.to_vec())
                };
                if sender.send(event).is_err() {
                    break;
                }
            }
            // Drop closes the receiver. No silent resubscription across a gap.
        });
        Ok(Box::new(RedisSubscription {
            receiver,
            task,
            initial: true,
        }))
    }
}
struct RedisSubscription {
    receiver: broadcast::Receiver<Notification>,
    task: JoinHandle<()>,
    initial: bool,
}
impl Drop for RedisSubscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[async_trait::async_trait]
impl Subscription for RedisSubscription {
    async fn recv(&mut self) -> Result<Notification> {
        if self.initial {
            self.initial = false;
            return Ok(Notification::ResyncRequired);
        }
        match self.receiver.recv().await {
            Ok(value) => Ok(value),
            Err(broadcast::error::RecvError::Lagged(_)) => Ok(Notification::ResyncRequired),
            Err(broadcast::error::RecvError::Closed) => Err(CacheError::Closed),
        }
    }
}
