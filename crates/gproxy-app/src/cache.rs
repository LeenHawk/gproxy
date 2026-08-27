use std::time::Duration;

use gproxy_core::CacheBackend;
use gproxy_core::channel_api::BoxFuture;

type Error = gproxy_core::error::StoreError;

#[cfg(not(target_arch = "wasm32"))]
type SharedCache = std::sync::Arc<dyn CacheBackend + Send + Sync>;
#[cfg(target_arch = "wasm32")]
type SharedCache = std::rc::Rc<dyn CacheBackend>;

#[derive(Clone)]
pub(crate) struct AppCache {
    inner: SharedCache,
}

impl AppCache {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new(cache: impl CacheBackend + Send + Sync + 'static) -> Self {
        Self {
            inner: std::sync::Arc::new(cache),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn new(cache: impl CacheBackend + 'static) -> Self {
        Self {
            inner: std::rc::Rc::new(cache),
        }
    }
}

impl CacheBackend for AppCache {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, Error>> {
        self.inner.get(key)
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), Error>> {
        self.inner.set(key, value, ttl)
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), Error>> {
        self.inner.delete(key)
    }

    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, Error>> {
        self.inner.incr(key, by, ttl)
    }

    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected_state: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, Error>> {
        self.inner
            .compare_incr_and_set(counter_key, by, state_key, expected_state, state)
    }

    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, Error>> {
        self.inner.compare_and_swap(key, expected, value, ttl)
    }
}
