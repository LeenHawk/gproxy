#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use gproxy_core::error::StoreError;
#[cfg(not(target_arch = "wasm32"))]
use gproxy_core::{Continuation, ContinuationKey, ContinuationMeta, ContinuationStore};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub(crate) struct LocalContinuations(Mutex<HashMap<ContinuationKey, Continuation>>);

#[cfg(not(target_arch = "wasm32"))]
impl ContinuationStore for LocalContinuations {
    fn peek(&self, key: &ContinuationKey) -> Result<Option<ContinuationMeta>, StoreError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| StoreError("continuation lock poisoned".into()))?
            .get(key)
            .map(Continuation::meta))
    }

    fn put(
        &self,
        value: Continuation,
    ) -> Result<Option<Continuation>, (StoreError, Box<Continuation>)> {
        let key = value.key().clone();
        match self.0.lock() {
            Ok(mut values) => Ok(values.insert(key, value)),
            Err(_) => Err((
                StoreError("continuation lock poisoned".into()),
                Box::new(value),
            )),
        }
    }

    fn take(&self, key: &ContinuationKey) -> Result<Option<Continuation>, StoreError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| StoreError("continuation lock poisoned".into()))?
            .remove(key))
    }

    fn take_generation(
        &self,
        key: &ContinuationKey,
        generation: &str,
    ) -> Result<Option<Continuation>, StoreError> {
        let mut values = self
            .0
            .lock()
            .map_err(|_| StoreError("continuation lock poisoned".into()))?;
        if values
            .get(key)
            .is_some_and(|value| value.meta().generation == generation)
        {
            Ok(values.remove(key))
        } else {
            Ok(None)
        }
    }
}
