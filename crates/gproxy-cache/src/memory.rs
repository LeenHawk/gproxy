use crate::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::sync::broadcast;
use web_time::Instant;

#[derive(Clone, Debug)]
pub struct MemoryOptions {
    /// Live keys across KV, counter and permit domains. A full cache sheds
    /// live values, then counters, soonest to expire first; never a permit.
    pub max_keys: usize,
    pub max_topics: usize,
    pub limits: Limits,
}
impl Default for MemoryOptions {
    fn default() -> Self {
        Self {
            max_keys: 100_000,
            max_topics: 256,
            limits: Limits::default(),
        }
    }
}
struct Timed<T> {
    value: T,
    expires: Instant,
}
#[derive(Default)]
struct State {
    values: HashMap<String, Timed<Entry>>,
    counters: HashMap<String, Timed<Counter>>,
    permits: HashMap<String, HashMap<Version, Instant>>,
    topics: HashMap<String, broadcast::Sender<Vec<u8>>>,
}
impl State {
    fn keys(&self) -> usize {
        self.values.len() + self.counters.len() + self.permits.len()
    }
    fn purge(&mut self, now: Instant) -> usize {
        let before = self.keys();
        self.values.retain(|_, v| v.expires > now);
        self.counters.retain(|_, v| v.expires > now);
        self.permits.retain(|_, holders| {
            holders.retain(|_, expires| *expires > now);
            !holders.is_empty()
        });
        self.topics.retain(|_, sender| sender.receiver_count() > 0);
        before - self.keys()
    }
    /// Make room for one more key.
    ///
    /// Expired keys go first. A cache still full after that sheds live soft
    /// state rather than refusing the write: values, then counters, soonest to
    /// expire first. Refusing would fail every request that writes a new key —
    /// an affinity pin after a paid upstream answer, a fresh rate-limit
    /// window — once enough distinct sessions have passed through. Permits are
    /// never shed: a lease is a promise of exclusion, and dropping one would
    /// let a second holder in.
    ///
    /// Eviction takes an eighth of the capacity at once, so a cache that stays
    /// full scans its keys once per `max / 8` inserts rather than on each.
    fn room(&mut self, max: usize, now: Instant) -> Result<()> {
        if self.keys() < max {
            return Ok(());
        }
        self.purge(now);
        let batch = (max / 8).max(1);
        if self.keys() >= max {
            evict_soonest(&mut self.values, batch);
        }
        if self.keys() >= max {
            evict_soonest(&mut self.counters, batch);
        }
        if self.keys() >= max {
            return Err(CacheError::Capacity);
        }
        Ok(())
    }
    fn expire_key(&mut self, key: &str, now: Instant) {
        if self.values.get(key).is_some_and(|v| v.expires <= now) {
            self.values.remove(key);
        }
        if self.counters.get(key).is_some_and(|v| v.expires <= now) {
            self.counters.remove(key);
        }
        if let Some(holders) = self.permits.get_mut(key) {
            holders.retain(|_, expires| *expires > now);
            if holders.is_empty() {
                self.permits.remove(key);
            }
        }
    }
}
/// Drop the `count` entries of `map` that expire soonest, or all of them.
fn evict_soonest<T>(map: &mut HashMap<String, Timed<T>>, count: usize) {
    if map.len() <= count {
        map.clear();
        return;
    }
    let mut expiries: Vec<Instant> = map.values().map(|v| v.expires).collect();
    let (_, cutoff, _) = expiries.select_nth_unstable(count - 1);
    let cutoff = *cutoff;
    map.retain(|_, v| v.expires > cutoff);
}
struct Inner {
    options: MemoryOptions,
    state: Mutex<State>,
}
/// Clone shares one process-local cache. Separate new() calls are isolated.
#[derive(Clone)]
pub struct MemoryCache(Arc<Inner>);
impl Default for MemoryCache {
    fn default() -> Self {
        Self::new(MemoryOptions::default()).expect("valid defaults")
    }
}
impl MemoryCache {
    pub fn new(options: MemoryOptions) -> Result<Self> {
        options.limits.validate()?;
        if options.max_keys == 0 || options.max_topics == 0 {
            return Err(CacheError::Invalid("memory capacities must be positive"));
        }
        Ok(Self(Arc::new(Inner {
            options,
            state: Mutex::new(State::default()),
        })))
    }
    fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.0.state.lock().map_err(|_| CacheError::Poisoned)
    }
    /// Optional maintenance; expiry is always enforced on access. No background task.
    pub fn purge_expired(&self) -> Result<usize> {
        Ok(self.state()?.purge(Instant::now()))
    }
    fn expiry(ttl: Duration) -> Result<Instant> {
        Instant::now()
            .checked_add(Duration::from_millis(ttl_ms(ttl)?))
            .ok_or(CacheError::Invalid("TTL exceeds clock range"))
    }
}
#[async_trait::async_trait]
impl Cache for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<Entry>> {
        self.0.options.limits.key(key)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        Ok(state.values.get(key).map(|v| v.value.clone()))
    }
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Entry>>> {
        for key in keys {
            self.0.options.limits.key(key)?;
        }
        let now = Instant::now();
        let mut state = self.state()?;
        Ok(keys
            .iter()
            .map(|key| {
                state.expire_key(key, now);
                state.values.get(key.as_str()).map(|v| v.value.clone())
            })
            .collect())
    }
    async fn put(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<Version> {
        self.0.options.limits.key(key)?;
        self.0.options.limits.value(&value)?;
        let expires = Self::expiry(ttl)?;
        let version = Version::fresh()?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        if !state.values.contains_key(key) {
            state.room(self.0.options.max_keys, Instant::now())?;
        }
        state.values.insert(
            key.into(),
            Timed {
                value: Entry { value, version },
                expires,
            },
        );
        Ok(version)
    }
    async fn delete(&self, key: &str) -> Result<bool> {
        self.0.options.limits.key(key)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        Ok(state.values.remove(key).is_some())
    }
    async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<Replacement>,
    ) -> Result<CasOutcome> {
        self.0.options.limits.key(key)?;
        let next = replacement
            .map(|r| -> Result<_> {
                self.0.options.limits.value(&r.value)?;
                Ok(Timed {
                    value: Entry {
                        value: r.value,
                        version: Version::fresh()?,
                    },
                    expires: Self::expiry(r.ttl)?,
                })
            })
            .transpose()?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        if state.values.get(key).map(|v| v.value.version) != expected {
            return Ok(CasOutcome::Conflict);
        }
        let version = next.as_ref().map(|v| v.value.version);
        if let Some(next) = next {
            if !state.values.contains_key(key) {
                state.room(self.0.options.max_keys, Instant::now())?;
            }
            state.values.insert(key.into(), next);
        } else {
            state.values.remove(key);
        }
        Ok(CasOutcome::Applied(version))
    }
    async fn counter(&self, key: &str) -> Result<Option<Counter>> {
        self.0.options.limits.key(key)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        Ok(state.counters.get(key).map(|v| v.value))
    }
    async fn increment(
        &self,
        key: &str,
        amount: u64,
        limit: u64,
        ttl: Duration,
    ) -> Result<IncrementOutcome> {
        self.0.options.limits.key(key)?;
        counter_args(amount, limit)?;
        let expires = Self::expiry(ttl)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        let current = state.counters.get(key).map_or(0, |v| v.value.value);
        if amount > limit.saturating_sub(current) {
            return Ok(IncrementOutcome::Limited { current });
        }
        if let Some(entry) = state.counters.get_mut(key) {
            entry.value.value += amount;
            return Ok(IncrementOutcome::Applied(entry.value));
        }
        state.room(self.0.options.max_keys, Instant::now())?;
        let counter = Counter {
            value: amount,
            generation: Version::fresh()?,
        };
        state.counters.insert(
            key.into(),
            Timed {
                value: counter,
                expires,
            },
        );
        Ok(IncrementOutcome::Applied(counter))
    }
    async fn decrement(
        &self,
        key: &str,
        generation: Version,
        amount: u64,
    ) -> Result<Option<Counter>> {
        self.0.options.limits.key(key)?;
        counter_args(amount, i64::MAX as u64)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        let Some(entry) = state
            .counters
            .get_mut(key)
            .filter(|v| v.value.generation == generation)
        else {
            return Ok(None);
        };
        if entry.value.value < amount {
            return Err(CacheError::Underflow);
        }
        entry.value.value -= amount;
        Ok(Some(entry.value))
    }
    async fn acquire_permit(
        &self,
        key: &str,
        limit: u32,
        ttl: Duration,
    ) -> Result<Option<Version>> {
        self.0.options.limits.key(key)?;
        self.0.options.limits.permits(limit)?;
        let expires = Self::expiry(ttl)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        if state
            .permits
            .get(key)
            .is_some_and(|v| v.len() >= limit as usize)
        {
            return Ok(None);
        }
        if !state.permits.contains_key(key) {
            state.room(self.0.options.max_keys, Instant::now())?;
        }
        let owner = Version::fresh()?;
        state
            .permits
            .entry(key.into())
            .or_default()
            .insert(owner, expires);
        Ok(Some(owner))
    }
    async fn renew_permit(&self, key: &str, owner: Version, ttl: Duration) -> Result<bool> {
        self.0.options.limits.key(key)?;
        let expires = Self::expiry(ttl)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        let Some(deadline) = state.permits.get_mut(key).and_then(|v| v.get_mut(&owner)) else {
            return Ok(false);
        };
        *deadline = expires;
        Ok(true)
    }
    async fn release_permit(&self, key: &str, owner: Version) -> Result<bool> {
        self.0.options.limits.key(key)?;
        let mut state = self.state()?;
        state.expire_key(key, Instant::now());
        let released = state
            .permits
            .get_mut(key)
            .is_some_and(|v| v.remove(&owner).is_some());
        if state.permits.get(key).is_some_and(HashMap::is_empty) {
            state.permits.remove(key);
        }
        Ok(released)
    }
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<()> {
        self.0.options.limits.key(topic)?;
        self.0.options.limits.value(&payload)?;
        if let Some(sender) = self.state()?.topics.get(topic) {
            let _ = sender.send(payload);
        }
        Ok(())
    }
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Subscription>> {
        self.0.options.limits.key(topic)?;
        let mut state = self.state()?;
        state.topics.retain(|_, sender| sender.receiver_count() > 0);
        if !state.topics.contains_key(topic) && state.topics.len() >= self.0.options.max_topics {
            return Err(CacheError::Capacity);
        }
        let sender = state
            .topics
            .entry(topic.into())
            .or_insert_with(|| broadcast::channel(self.0.options.limits.notification_capacity).0);
        Ok(Box::new(MemorySubscription {
            receiver: sender.subscribe(),
            initial: true,
        }))
    }
}
struct MemorySubscription {
    receiver: broadcast::Receiver<Vec<u8>>,
    initial: bool,
}
#[async_trait::async_trait]
impl Subscription for MemorySubscription {
    async fn recv(&mut self) -> Result<Notification> {
        if self.initial {
            self.initial = false;
            return Ok(Notification::ResyncRequired);
        }
        match self.receiver.recv().await {
            Ok(value) => Ok(Notification::Message(value)),
            Err(broadcast::error::RecvError::Lagged(_)) => Ok(Notification::ResyncRequired),
            Err(broadcast::error::RecvError::Closed) => Err(CacheError::Closed),
        }
    }
}
