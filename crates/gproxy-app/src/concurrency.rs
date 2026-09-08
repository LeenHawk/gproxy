use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

pub struct ConcurrencyLimit {
    state: Mutex<State>,
    changed: Notify,
}

struct State {
    active: usize,
    limit: usize,
}

impl ConcurrencyLimit {
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State { active: 0, limit }),
            changed: Notify::new(),
        })
    }

    pub fn set_limit(&self, limit: usize) {
        let mut state = self.state.lock().expect("concurrency state poisoned");
        if state.limit != limit {
            state.limit = limit;
            self.changed.notify_waiters();
        }
    }

    pub async fn acquire(self: &Arc<Self>) -> ConcurrencyPermit {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.state.lock().expect("concurrency state poisoned");
                // Unlimited admissions still count, so a later finite limit sees
                // requests that started before the setting changed.
                if state.limit == 0 || state.active < state.limit {
                    state.active += 1;
                    return ConcurrencyPermit(Arc::clone(self));
                }
            }
            changed.await;
        }
    }
}

pub struct ConcurrencyPermit(Arc<ConcurrencyLimit>);

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .expect("concurrency state poisoned")
            .active -= 1;
        self.0.changed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[tokio::test]
    async fn hot_limits_preserve_active_requests_and_wake_waiters() {
        let gate = super::ConcurrencyLimit::new(0);
        let first = gate.acquire().await;
        let second = gate.acquire().await;
        gate.set_limit(1);
        let waiting = gate.acquire();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        drop(first);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        gate.set_limit(2);
        let third = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap();
        drop(second);
        drop(third);
        gate.set_limit(1);
        let last = gate.acquire().await;
        let waiting = gate.acquire();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        drop(last);
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap();
    }
}
