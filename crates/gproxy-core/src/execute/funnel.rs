//! One request's settlement funnel. `Settled` is a proof that the response
//! handed to the caller is armed to finish this funnel; only the funnel mints it.

use crate::{
    ExchangeUsage, ObservationPolicy, Observer, RequestContext, Settled, TraceEvent,
    UsageCompletion, UsageReport, UsageState,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

pub(crate) struct Funnel {
    request: Arc<RequestContext>,
    policy: ObservationPolicy,
    observer: Arc<dyn Observer>,
    exchanges: Mutex<Vec<ExchangeUsage>>,
    finished: AtomicBool,
    handed_off: AtomicBool,
    exchanges_closed: Mutex<Vec<oneshot::Receiver<()>>>,
    sender: Mutex<Option<oneshot::Sender<crate::CoreResult<UsageReport>>>>,
    /// Charges settled usage against Counted dimensions before the report leaves.
    meter: Mutex<Option<Arc<dyn crate::quota::UsageMeter>>>,
    realtime_dedup: Mutex<Option<crate::realtime::SettlementDedup>>,
}

impl Funnel {
    pub fn new(
        request: Arc<RequestContext>,
        observer: Arc<dyn Observer>,
    ) -> (Arc<Self>, UsageCompletion) {
        let policy = observer.policy(&request);
        let (sender, receiver) = oneshot::channel();
        let funnel = Arc::new(Self {
            request,
            policy,
            observer,
            exchanges: Mutex::new(Vec::new()),
            finished: AtomicBool::new(false),
            handed_off: AtomicBool::new(false),
            exchanges_closed: Mutex::new(Vec::new()),
            sender: Mutex::new(Some(sender)),
            meter: Mutex::new(None),
            realtime_dedup: Mutex::new(None),
        });
        let completion: UsageCompletion =
            Box::pin(async move { receiver.await.unwrap_or(Err(crate::CoreError::Cancelled)) });
        (funnel, completion)
    }

    pub fn open_exchange(&self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        self.exchanges_closed.lock().unwrap().push(rx);
        tx
    }
    pub fn handed_off(&self) -> bool {
        self.handed_off.load(Ordering::SeqCst)
    }

    pub fn interrupted_state(&self) -> UsageState {
        if self.request.cancellation.is_cancelled() {
            UsageState::Cancelled
        } else {
            UsageState::Failed
        }
    }

    pub fn set_meter(&self, meter: Arc<dyn crate::quota::UsageMeter>) {
        *self.meter.lock().unwrap() = Some(meter);
    }

    pub fn set_realtime_dedup(&self, dedup: crate::realtime::SettlementDedup) {
        *self.realtime_dedup.lock().unwrap() = Some(dedup);
    }

    pub fn policy(&self) -> ObservationPolicy {
        self.policy
    }
    pub fn observer(&self) -> &Arc<dyn Observer> {
        &self.observer
    }

    pub fn trace(&self, event: TraceEvent<'_>) {
        if self.policy.trace {
            self.observer.trace(event);
        }
    }

    pub fn record_exchange_usage(&self, usage: ExchangeUsage) {
        self.exchanges.lock().unwrap().push(usage);
    }

    /// The response body/socket handed to the caller will call `finish` when it
    /// ends. Minting the proof here keeps "returned to caller" and "will
    /// settle" the same event.
    pub fn arm(&self) -> Settled {
        self.handed_off.store(true, Ordering::SeqCst);
        Settled(())
    }

    pub fn guard(self: &Arc<Self>) -> RequestGuard {
        RequestGuard(self.clone())
    }

    /// Exactly once per request. Later calls are no-ops, so a body ending
    /// after an explicit failure does not settle twice.
    pub async fn finish(self: &Arc<Self>, state: UsageState) -> Settled {
        // Once settlement starts, dropping its waiter must not cancel DB writes.
        let this = self.clone();
        let (tx, rx) = oneshot::channel();
        crate::rt::spawn(async move {
            this.finish_inner(state).await;
            let _ = tx.send(());
        });
        let _ = rx.await;
        Settled(())
    }
    async fn finish_inner(&self, state: UsageState) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        // Capture flush and synchronous usage observation for every physical
        // call precede the single request summary, including dropped bodies.
        let pending = std::mem::take(&mut *self.exchanges_closed.lock().unwrap());
        for closed in pending {
            let _ = closed.await;
        }
        let mut report = UsageReport {
            request_id: self.request.request_id.clone(),
            downstream_usage: None,
            exchanges: std::mem::take(&mut *self.exchanges.lock().unwrap()),
            cost: None,
            state: if self.policy.usage {
                state
            } else {
                UsageState::Skipped
            },
        };
        if self.policy.usage {
            let dedup = self.realtime_dedup.lock().unwrap().clone();
            if let Some(dedup) = dedup
                && let Err(error) = dedup.filter(&self.request, &mut report).await
            {
                // Fail closed on shared-ledger errors: do not charge an
                // aggregate whose response ownership could not be established.
                report.exchanges.clear();
                report.state = UsageState::Failed;
                self.observer.usage(&self.request, &report).await;
                if let Some(sender) = self.sender.lock().unwrap().take() {
                    let _ = sender.send(Err(error));
                }
                return;
            }
            // Cost is computed here, once, so the meter's budget settlement
            // and the Observer's record agree on the number.
            if self.request.snapshot.observation.settlement {
                crate::pricing::price_report(
                    &self.request.snapshot.pricing,
                    self.request.operation.operation,
                    &mut report,
                );
                let meter = self.meter.lock().unwrap().clone();
                if let Some(meter) = meter
                    && !report.exchanges.is_empty()
                {
                    meter.charge(&self.request, &report).await;
                }
            }
            self.observer.usage(&self.request, &report).await;
        }
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(Ok(report));
        }
    }

    /// From Drop paths, where nothing can be awaited. Requires a Tokio runtime;
    /// without one the funnel cannot run and the completion resolves Cancelled.
    pub fn finish_detached(self: Arc<Self>, state: UsageState) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        crate::rt::spawn(async move {
            self.finish(state).await;
        });
    }
}

/// Covers cancellation by dropping the execution future before response handoff.
pub(crate) struct RequestGuard(Arc<Funnel>);
impl Drop for RequestGuard {
    fn drop(&mut self) {
        if !self.0.handed_off.load(Ordering::SeqCst) {
            self.0.clone().finish_detached(UsageState::Cancelled);
        }
    }
}
