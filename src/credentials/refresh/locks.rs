use std::sync::Arc;

use dashmap::DashMap;
use futures_util::lock::Mutex;

pub(super) struct RefreshLocks {
    by_credential: DashMap<i64, Arc<Mutex<()>>>,
}

impl RefreshLocks {
    pub(super) fn new() -> Self {
        Self {
            by_credential: DashMap::new(),
        }
    }

    pub(super) fn for_credential(&self, credential_id: i64) -> Arc<Mutex<()>> {
        self.by_credential
            .entry(credential_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
