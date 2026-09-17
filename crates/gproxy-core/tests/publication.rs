#![cfg(not(target_arch = "wasm32"))]

use gproxy_core::{
    CapturePolicy, CaptureSink, ConfigRevision, Core, CoreData, CredentialState, CredentialVersion,
    ExchangeContext, ObservationPolicy, Observer, RequestContext, TraceEvent, UsageReport,
};
use gproxy_protocol::capability::CapabilityFuture;
use std::sync::{Arc, Barrier};

/// An explicit host choice to observe nothing; core provides no such default.
struct ObserveNothing;
impl Observer for ObserveNothing {
    fn policy(&self, _: &RequestContext) -> ObservationPolicy {
        ObservationPolicy {
            usage: false,
            capture: CapturePolicy::Off,
            trace: false,
        }
    }
    fn capture(&self, _: &ExchangeContext, _: CapturePolicy) -> Box<dyn CaptureSink> {
        unreachable!("capture is never opened under CapturePolicy::Off")
    }
    fn usage<'a>(&'a self, _: &'a UsageReport) -> CapabilityFuture<'a, ()> {
        Box::pin(async {})
    }
    fn trace(&self, _: TraceEvent<'_>) {}
}

#[test]
fn concurrent_reload_never_regresses_and_inflight_snapshot_stays_pinned() {
    let core = Arc::new(Core::new(
        Arc::new(gproxy_store::Store::new(())),
        Arc::new(gproxy_cache::MemoryCache::default()),
        Arc::new(ObserveNothing),
        Arc::new(CoreData::default()),
    ));
    let pinned = core.snapshot();
    let barrier = Arc::new(Barrier::new(16));
    std::thread::scope(|scope| {
        for revision in 1..=16 {
            let core = core.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                let next = Arc::new(CoreData {
                    revision: ConfigRevision(revision),
                    ..Default::default()
                });
                barrier.wait();
                core.publish_snapshot(next);
            });
        }
    });
    assert_eq!(core.snapshot().revision, ConfigRevision(16));
    assert_eq!(pinned.revision, ConfigRevision(0));
    let current = core.snapshot();
    assert!(!core.publish_snapshot(Arc::new(CoreData {
        revision: ConfigRevision(16),
        ..Default::default()
    })));
    assert!(!core.publish_snapshot(Arc::new(CoreData {
        revision: ConfigRevision(2),
        ..Default::default()
    })));
    assert!(Arc::ptr_eq(&current, &core.snapshot()));
}

#[test]
fn refreshed_material_is_atomic_and_old_attempt_cannot_observe_half_a_version() {
    let material = |version| {
        Arc::new(CredentialVersion {
            version,
            expires_at_ms: Some(version * 100),
            secret: serde_json::json!({"version": version}),
        })
    };
    let state = Arc::new(CredentialState::new(material(0)));
    let pinned = state.load();
    std::thread::scope(|scope| {
        for version in 1..=16 {
            let state = state.clone();
            let next = material(version);
            scope.spawn(move || {
                state.publish_if_newer(next);
            });
        }
    });
    let current = state.load();
    assert_eq!(current.version, 16);
    assert_eq!(current.expires_at_ms, Some(1600));
    assert_eq!(current.secret["version"], 16);
    assert_eq!(pinned.version, 0);
    assert_eq!(pinned.secret["version"], 0);
    assert!(!state.publish_if_newer(material(1)));
    assert!(!state.publish_if_newer(material(16)));
    assert!(Arc::ptr_eq(&current, &state.load()));
}
