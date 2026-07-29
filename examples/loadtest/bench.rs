//! Request driving: execute one pipeline call and fully consume the body;
//! run a fixed-concurrency load window over one request template.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::StreamExt as _;
use gproxy::app::AppState;
use gproxy::pipeline::{RequestCtx, ResponseBody};

use crate::matrix::RequestTemplate;
use crate::metrics::{Counters, RssSampler, kb_to_mb, rss_kb};

/// Monotonic request-id source shared by every worker.
pub static REQ_SEQ: AtomicU64 = AtomicU64::new(0);

/// Execute one request and drain the response. Ok(bytes relayed) only for a
/// 2xx with a fully consumed body.
pub async fn drive(state: &AppState, ctx: RequestCtx) -> Result<u64, String> {
    let outcome = gproxy::pipeline::execute(state, ctx)
        .await
        .map_err(|e| e.to_string())?;
    let status = outcome.status;
    let mut relayed = 0u64;
    match outcome.body {
        ResponseBody::Full(b) => relayed += b.len() as u64,
        ResponseBody::Stream(mut s) => {
            while let Some(chunk) = s.next().await {
                relayed += chunk.map_err(|e| e.to_string())?.len() as u64;
            }
        }
    }
    if status.is_success() {
        Ok(relayed)
    } else {
        Err(format!("status {status}"))
    }
}

pub struct LoadResult {
    pub req_s: f64,
    pub mb_s: f64,
    /// Peak RSS during the measured window minus RSS at window start (MB).
    pub peak_delta_mb: f64,
    /// Absolute peak RSS during the measured window (KiB).
    pub peak_kb: u64,
    pub errors: u64,
}

/// Run `concurrency` workers over `tpl`: warm up, measure a timed window, stop,
/// then drain detached settle tasks (~500ms) before reading the final counters.
pub async fn run_load(
    state: &Arc<AppState>,
    tpl: &Arc<RequestTemplate>,
    concurrency: usize,
    warmup: Duration,
    window: Duration,
    sampler: &RssSampler,
) -> LoadResult {
    let stop = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::default());
    // Backpressure: buffered content-gen settles are DETACHED spawns
    // (`failover::mod.rs` §17); with every core saturated by all-ready mock
    // futures the backlog grows without bound (OOM). Cap the total alive-task
    // count so workers pause until the settle tail drains.
    let task_cap = concurrency * 4 + 512;
    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let (state, tpl, stop, counters) = (
            Arc::clone(state),
            Arc::clone(tpl),
            Arc::clone(&stop),
            Arc::clone(&counters),
        );
        handles.push(tokio::spawn(async move {
            let metrics = tokio::runtime::Handle::current().metrics();
            while !stop.load(Ordering::Relaxed) {
                while metrics.num_alive_tasks() > task_cap {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                let ctx = tpl.ctx(REQ_SEQ.fetch_add(1, Ordering::Relaxed));
                match drive(&state, ctx).await {
                    Ok(bytes) => {
                        counters.requests.fetch_add(1, Ordering::Relaxed);
                        counters.bytes.fetch_add(bytes, Ordering::Relaxed);
                    }
                    Err(_) => {
                        counters.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    tokio::time::sleep(warmup).await;
    sampler.reset();
    let rss_start = rss_kb();
    let (r0, b0, _) = counters.snapshot();
    tokio::time::sleep(window).await;
    let (r1, b1, _) = counters.snapshot();
    let peak_kb = sampler.peak_kb();

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.await;
    }
    // Settle runs as detached spawns — let the tail drain before final reads.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (_, _, errors) = counters.snapshot();

    let secs = window.as_secs_f64();
    LoadResult {
        req_s: (r1 - r0) as f64 / secs,
        mb_s: (b1 - b0) as f64 / (1024.0 * 1024.0) / secs,
        peak_delta_mb: kb_to_mb(peak_kb.saturating_sub(rss_start)),
        peak_kb,
        errors,
    }
}
