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
    sender: Mutex<Option<oneshot::Sender<crate::CoreResult<UsageReport>>>>,
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
            sender: Mutex::new(Some(sender)),
        });
        let completion: UsageCompletion =
            Box::pin(async move { receiver.await.unwrap_or(Err(crate::CoreError::Cancelled)) });
        (funnel, completion)
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
        Settled(())
    }

    /// Exactly once per request. Later calls are no-ops, so a body ending
    /// after an explicit failure does not settle twice.
    pub async fn finish(&self, state: UsageState) -> Settled {
        if self.finished.swap(true, Ordering::SeqCst) {
            return Settled(());
        }
        let report = UsageReport {
            request_id: self.request.request_id.clone(),
            downstream_usage: None,
            exchanges: std::mem::take(&mut *self.exchanges.lock().unwrap()),
            state: if self.policy.usage {
                state
            } else {
                UsageState::Skipped
            },
        };
        if self.policy.usage {
            self.observer.usage(&report).await;
        }
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(Ok(report));
        }
        Settled(())
    }

    /// From Drop paths, where nothing can be awaited. Requires a Tokio runtime;
    /// without one the funnel cannot run and the completion resolves Cancelled.
    pub fn finish_detached(self: Arc<Self>, state: UsageState) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                self.finish(state).await;
            });
        }
    }
}
