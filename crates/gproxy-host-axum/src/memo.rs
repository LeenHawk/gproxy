//! Values derived from published snapshots, built once per snapshot instead
//! of once per request.
//!
//! A snapshot is identified by its allocation, not by a revision number: the
//! memo keeps a `Weak` to it, which keeps the allocation from being reused
//! while the memo remembers it, so an address match can only be the same
//! snapshot. Holding the `Weak` rather than an `Arc` lets a replaced
//! snapshot's contents go as soon as the requests using it end.

use std::sync::{Arc, Mutex, Weak};

/// The snapshots a derived value depends on.
pub(crate) trait SnapshotKey {
    type Weak: Send;
    fn downgrade(&self) -> Self::Weak;
    fn is(&self, weak: &Self::Weak) -> bool;
}

impl<A> SnapshotKey for Arc<A>
where
    Weak<A>: Send,
{
    type Weak = Weak<A>;
    fn downgrade(&self) -> Weak<A> {
        Arc::downgrade(self)
    }
    fn is(&self, weak: &Weak<A>) -> bool {
        std::ptr::eq(Arc::as_ptr(self), weak.as_ptr())
    }
}

impl<A: SnapshotKey, B: SnapshotKey> SnapshotKey for (A, B) {
    type Weak = (A::Weak, B::Weak);
    fn downgrade(&self) -> Self::Weak {
        (self.0.downgrade(), self.1.downgrade())
    }
    fn is(&self, weak: &Self::Weak) -> bool {
        self.0.is(&weak.0) && self.1.is(&weak.1)
    }
}

/// The value derived from the latest snapshot seen. Only one is kept: every
/// request uses the current snapshot but for the few in flight across a
/// publish, and those rebuild at worst once each.
pub(crate) struct Memo<K: SnapshotKey, V> {
    slot: Mutex<Option<(K::Weak, Arc<V>)>>,
}

impl<K: SnapshotKey, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }
}

impl<K: SnapshotKey, V> Memo<K, V> {
    /// The value for `key`, built by `build` unless it already was. Building
    /// happens outside the lock, so a slow build never stalls a request that
    /// only reads.
    pub(crate) fn get(&self, key: &K, build: impl FnOnce() -> V) -> Arc<V> {
        if let Some((weak, value)) = &*self.slot.lock().unwrap()
            && key.is(weak)
        {
            return value.clone();
        }
        let value = Arc::new(build());
        *self.slot.lock().unwrap() = Some((key.downgrade(), value.clone()));
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_built_once_per_snapshot() {
        let memo = Memo::<Arc<u32>, u32>::default();
        let first = Arc::new(1);
        let built = std::cell::Cell::new(0);
        let build = |n: u32| {
            built.set(built.get() + 1);
            n * 10
        };
        assert_eq!(*memo.get(&first, || build(1)), 10);
        assert_eq!(*memo.get(&first, || build(1)), 10);
        assert_eq!(built.get(), 1);
        // An equal value in another allocation is another snapshot.
        let second = Arc::new(1);
        assert_eq!(*memo.get(&second, || build(2)), 20);
        assert_eq!(built.get(), 2);
    }
}
