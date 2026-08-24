use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gproxy_core::CacheBackend;
use gproxy_core::channel_api::BoxFuture;

#[derive(Clone, Default)]
pub(crate) struct InProcessCache {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

struct Entry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

impl CacheBackend for InProcessCache {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, gproxy_core::error::StoreError>> {
        let result = self.with_entries(|entries| {
            expire(entries, key);
            Ok(entries.get(key).map(|entry| entry.value.clone()))
        });
        Box::pin(async move { result })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), gproxy_core::error::StoreError>> {
        let result = self.with_entries(|entries| {
            entries.insert(
                key.to_owned(),
                Entry {
                    value,
                    expires_at: expiry(ttl)?,
                },
            );
            Ok(())
        });
        Box::pin(async move { result })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<(), gproxy_core::error::StoreError>> {
        let result = self.with_entries(|entries| {
            entries.remove(key);
            Ok(())
        });
        Box::pin(async move { result })
    }

    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, gproxy_core::error::StoreError>> {
        let result = self.with_entries(|entries| {
            expire(entries, key);
            let (current, expires_at) = match entries.get(key) {
                Some(entry) => (decode_counter(&entry.value)?, entry.expires_at),
                None => (0, expiry(ttl)?),
            };
            let value = current
                .checked_add(by)
                .ok_or_else(|| gproxy_core::error::StoreError("cache counter overflow".into()))?;
            entries.insert(
                key.to_owned(),
                Entry {
                    value: value.to_be_bytes().to_vec(),
                    expires_at,
                },
            );
            Ok(value)
        });
        Box::pin(async move { result })
    }
}

impl InProcessCache {
    fn with_entries<T>(
        &self,
        operation: impl FnOnce(&mut HashMap<String, Entry>) -> Result<T, gproxy_core::error::StoreError>,
    ) -> Result<T, gproxy_core::error::StoreError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| gproxy_core::error::StoreError("cache lock poisoned".into()))?;
        operation(&mut entries)
    }
}

fn expire(entries: &mut HashMap<String, Entry>, key: &str) {
    if entries
        .get(key)
        .and_then(|entry| entry.expires_at)
        .is_some_and(|expires_at| expires_at <= Instant::now())
    {
        entries.remove(key);
    }
}

fn decode_counter(value: &[u8]) -> Result<i64, gproxy_core::error::StoreError> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| gproxy_core::error::StoreError("cache value is not a counter".into()))?;
    Ok(i64::from_be_bytes(bytes))
}

fn expiry(ttl: Option<Duration>) -> Result<Option<Instant>, gproxy_core::error::StoreError> {
    ttl.map(|ttl| {
        Instant::now()
            .checked_add(ttl)
            .ok_or_else(|| gproxy_core::error::StoreError("cache TTL exceeds clock range".into()))
    })
    .transpose()
}
