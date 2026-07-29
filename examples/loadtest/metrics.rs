//! RSS sampling (`/proc/self/status` VmRSS) and load counters.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Current resident set size in KiB (0 when unavailable).
pub fn rss_kb() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kb| kb.parse().ok())
        .unwrap_or(0)
}

pub fn kb_to_mb(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

/// Background task sampling peak RSS every 100ms.
pub struct RssSampler {
    peak: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
}

impl RssSampler {
    pub fn start() -> Self {
        let peak = Arc::new(AtomicU64::new(rss_kb()));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (Arc::clone(&peak), Arc::clone(&stop));
        let handle = tokio::spawn(async move {
            while !s.load(Ordering::Relaxed) {
                p.fetch_max(rss_kb(), Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        Self { peak, stop, handle }
    }

    /// Restart the peak from the current RSS (call at each window start).
    pub fn reset(&self) {
        self.peak.store(rss_kb(), Ordering::Relaxed);
    }

    pub fn peak_kb(&self) -> u64 {
        self.peak.load(Ordering::Relaxed).max(rss_kb())
    }

    pub async fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.await;
    }
}

/// Shared worker counters (requests / relayed bytes / failures).
#[derive(Default)]
pub struct Counters {
    pub requests: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
}

impl Counters {
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        )
    }
}
