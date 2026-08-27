use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gproxy_core::CacheBackend;
use gproxy_core::channel_api::BoxFuture;
use web_time::Instant;

type Error = gproxy_core::error::StoreError;

fn cache_error(message: &str) -> Error {
    gproxy_core::error::StoreError(message.into())
}

#[derive(Clone, Default)]
pub struct InProcessCache {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

struct Entry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

impl CacheBackend for InProcessCache {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, Error>> {
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
    ) -> BoxFuture<'a, Result<(), Error>> {
        let result = self.with_entries(|entries| {
            entries.insert(
                key.into(),
                Entry {
                    value,
                    expires_at: expiry(ttl)?,
                },
            );
            Ok(())
        });
        Box::pin(async move { result })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), Error>> {
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
    ) -> BoxFuture<'a, Result<i64, Error>> {
        let result = self.with_entries(|entries| {
            expire(entries, key);
            let (current, expires_at) = match entries.get(key) {
                Some(entry) => (decode_counter(&entry.value)?, entry.expires_at),
                None => (0, expiry(ttl)?),
            };
            let value = current.checked_add(by).ok_or_else(overflow)?;
            entries.insert(
                key.into(),
                Entry {
                    value: value.to_be_bytes().to_vec(),
                    expires_at,
                },
            );
            Ok(value)
        });
        Box::pin(async move { result })
    }

    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, Error>> {
        let result = self.with_entries(|entries| {
            expire(entries, state_key);
            if entries.get(state_key).map(|entry| &entry.value) != Some(&expected) {
                return Ok(None);
            }
            let current = entries
                .get(counter_key)
                .map_or(Ok(0), |entry| decode_counter(&entry.value))?;
            let next = current.checked_add(by).ok_or_else(overflow)?;
            entries.insert(
                counter_key.into(),
                Entry {
                    value: next.to_be_bytes().to_vec(),
                    expires_at: None,
                },
            );
            entries.insert(
                state_key.into(),
                Entry {
                    value: state,
                    expires_at: None,
                },
            );
            Ok(Some(next))
        });
        Box::pin(async move { result })
    }

    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, Error>> {
        let result = self.with_entries(|entries| {
            expire(entries, key);
            if entries.get(key).map(|entry| &entry.value) != expected.as_ref() {
                return Ok(false);
            }
            match value {
                Some(value) => {
                    entries.insert(
                        key.into(),
                        Entry {
                            value,
                            expires_at: expiry(ttl)?,
                        },
                    );
                }
                None => {
                    entries.remove(key);
                }
            }
            Ok(true)
        });
        Box::pin(async move { result })
    }
}

impl InProcessCache {
    fn with_entries<T>(
        &self,
        operation: impl FnOnce(&mut HashMap<String, Entry>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| cache_error("cache lock poisoned"))?;
        operation(&mut entries)
    }
}

fn expire(entries: &mut HashMap<String, Entry>, key: &str) {
    if entries
        .get(key)
        .and_then(|entry| entry.expires_at)
        .is_some_and(|expiry| expiry <= Instant::now())
    {
        entries.remove(key);
    }
}

fn expiry(ttl: Option<Duration>) -> Result<Option<Instant>, Error> {
    ttl.map(|ttl| {
        Instant::now()
            .checked_add(ttl)
            .ok_or_else(|| cache_error("cache TTL exceeds clock range"))
    })
    .transpose()
}

fn decode_counter(value: &[u8]) -> Result<i64, Error> {
    value
        .try_into()
        .map(i64::from_be_bytes)
        .map_err(|_| cache_error("cache value is not a counter"))
}

fn overflow() -> Error {
    cache_error("cache counter overflow")
}
