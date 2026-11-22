//! All state the engine has. The host supplies one key-value interface with
//! compare-and-exchange ([`KeyValue`], the contract of
//! `gproxy_upstream::UpstreamState`); credentials, sign-in sessions, channel
//! state and protocol continuation state are key prefixes on top of it.
//!
//! Expiry is the engine's: every value carries its own deadline, so a host
//! that cannot expire keys is still correct. The deadline is also passed down
//! as `StateWrite::expires_at`, which a host may use to reclaim the key.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use gproxy_protocol::{
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, CasResult, StateEntry, StateStore, StateWrite, Version,
    },
    connection::Bytes,
};

/// What a host implements: `get(key)` and
/// `compare_exchange(key, expected_version, replacement)`.
pub use gproxy_upstream::UpstreamState as KeyValue;

pub(crate) const LIMITS: CapabilityLimits = CapabilityLimits {
    operation_total: Duration::from_secs(1200),
    stream_idle: Duration::from_secs(300),
    read_bytes: 256 * 1024 * 1024,
    write_bytes: crate::MAX_REQUEST_BYTES,
    ws_frame_bytes: 32 * 1024 * 1024,
};

/// The deadline in front of every stored value: milliseconds since the epoch,
/// big-endian, zero for none.
const HEADER: usize = 8;

fn millis(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

fn seal(write: StateWrite) -> StateWrite {
    let deadline = write.expires_at.map_or(0, millis);
    let mut payload = Vec::with_capacity(HEADER + write.payload.len());
    payload.extend_from_slice(&deadline.to_be_bytes());
    payload.extend_from_slice(&write.payload);
    StateWrite {
        payload: Bytes::from(payload),
        expires_at: write.expires_at,
    }
}

/// The stored entry without its header, or `None` once its deadline passed.
fn open(entry: StateEntry) -> Result<Option<StateEntry>, CapabilityError> {
    let header: [u8; HEADER] = entry
        .payload
        .get(..HEADER)
        .and_then(|header| header.try_into().ok())
        .ok_or_else(|| storage("a stored value is shorter than its header"))?;
    let deadline = i64::from_be_bytes(header);
    if deadline != 0 && deadline <= gproxy_convert::now_ms() {
        return Ok(None);
    }
    Ok(Some(StateEntry {
        payload: entry.payload.slice(HEADER..),
        version: entry.version,
        expires_at: (deadline != 0)
            .then(|| SystemTime::UNIX_EPOCH + Duration::from_millis(deadline as u64)),
    }))
}

pub(crate) fn storage(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::Start,
        message,
    )
}

#[derive(Clone)]
pub(crate) struct Store {
    host: Arc<dyn KeyValue>,
}

impl Store {
    pub fn new(host: Arc<dyn KeyValue>) -> Self {
        Self { host }
    }

    pub async fn get(&self, key: &str) -> Result<Option<StateEntry>, CapabilityError> {
        match self.host.get(key).await? {
            Some(entry) => open(entry),
            None => Ok(None),
        }
    }

    /// `expected = None` requires absence, and an expired value is absent: it
    /// is replaced under the version it still has in the host's store.
    pub async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> Result<CasResult, CapabilityError> {
        let replacement = replacement.map(seal);
        if expected.is_some() {
            return self.host.compare_exchange(key, expected, replacement).await;
        }
        let created = self
            .host
            .compare_exchange(key, None, replacement.clone())
            .await?;
        if created != CasResult::Conflict {
            return Ok(created);
        }
        match self.host.get(key).await? {
            Some(stale) if open(stale.clone())?.is_none() => {
                self.host
                    .compare_exchange(key, Some(stale.version), replacement)
                    .await
            }
            _ => Ok(CasResult::Conflict),
        }
    }

    /// The same store under a key prefix, as a channel's cross-request state.
    pub fn scoped(&self, prefix: String) -> Scoped {
        Scoped {
            store: self.clone(),
            prefix,
        }
    }
}

pub(crate) struct Scoped {
    store: Store,
    prefix: String,
}

impl KeyValue for Scoped {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move { self.store.get(&format!("{}{key}", self.prefix)).await })
    }

    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            self.store
                .compare_exchange(&format!("{}{key}", self.prefix), expected, replacement)
                .await
        })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}

/// Protocol's continuation state. The scope is the key prefix.
#[derive(Clone)]
pub(crate) struct ProtocolState(pub Store);

impl StateStore for ProtocolState {
    type Scope = String;

    fn get<'a>(
        &'a self,
        scope: &'a String,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move { self.0.get(&format!("{scope}{key}")).await })
    }

    fn compare_exchange<'a>(
        &'a self,
        scope: &'a String,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            self.0
                .compare_exchange(&format!("{scope}{key}"), expected, replacement)
                .await
        })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}

/// A key-value store in memory: what a single process and the tests use.
#[derive(Default)]
pub struct MemoryState {
    entries: Mutex<Entries>,
}

#[derive(Default)]
struct Entries {
    values: HashMap<String, (Bytes, u64)>,
    /// Never reused, so a deleted and recreated key has a new version.
    last_version: u64,
}

fn version(number: u64) -> Version {
    Version::from_bytes(number.to_be_bytes())
}

impl KeyValue for MemoryState {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        let entries = self.entries.lock().expect("state lock");
        let entry = entries.values.get(key).map(|(payload, number)| StateEntry {
            payload: payload.clone(),
            version: version(*number),
            expires_at: None,
        });
        Box::pin(async move { Ok(entry) })
    }

    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        let mut entries = self.entries.lock().expect("state lock");
        let current = entries.values.get(key).map(|(_, number)| version(*number));
        let result = if current != expected {
            CasResult::Conflict
        } else if let Some(write) = replacement {
            entries.last_version += 1;
            let number = entries.last_version;
            entries
                .values
                .insert(key.to_owned(), (write.payload, number));
            CasResult::Applied(Some(version(number)))
        } else {
            entries.values.remove(key);
            CasResult::Applied(None)
        };
        Box::pin(async move { Ok(result) })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(payload: &'static str, expires_at: Option<SystemTime>) -> Option<StateWrite> {
        Some(StateWrite {
            payload: Bytes::from_static(payload.as_bytes()),
            expires_at,
        })
    }

    #[tokio::test]
    async fn an_expired_value_is_absent_and_can_be_created_over() {
        let store = Store::new(Arc::new(MemoryState::default()));
        let past = SystemTime::now() - Duration::from_secs(1);
        assert!(matches!(
            store
                .compare_exchange("k", None, write("old", Some(past)))
                .await,
            Ok(CasResult::Applied(Some(_)))
        ));
        assert_eq!(store.get("k").await.unwrap(), None);
        // Absent to a reader, so a writer expecting absence succeeds.
        assert!(matches!(
            store.compare_exchange("k", None, write("new", None)).await,
            Ok(CasResult::Applied(Some(_)))
        ));
        let entry = store.get("k").await.unwrap().unwrap();
        assert_eq!(&entry.payload[..], b"new");
        // A live value is not.
        assert_eq!(
            store
                .compare_exchange("k", None, write("again", None))
                .await
                .unwrap(),
            CasResult::Conflict
        );
        // And a stale version loses.
        let stale = entry.version.clone();
        store
            .compare_exchange("k", Some(entry.version), write("third", None))
            .await
            .unwrap();
        assert_eq!(
            store
                .compare_exchange("k", Some(stale), None)
                .await
                .unwrap(),
            CasResult::Conflict
        );
    }
}
