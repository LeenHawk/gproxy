//! Cross-request memory for a channel, scoped by the host to one provider
//! and one credential. Channels stay free of hidden state: whatever a vendor
//! protocol needs to remember between calls (a web conversation to resume, a
//! session to reuse) is written here under a key the channel chooses, and the
//! host decides where it lives, how long it may live and who else can see it
//! (nobody). The contract mirrors protocol's `StateStore` without the scope.

use gproxy_protocol::capability::{
    CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture, CapabilityLimits,
    CasResult, StateEntry, StateWrite, Version,
};

#[cfg(not(target_arch = "wasm32"))]
pub trait StateBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> StateBounds for T {}
#[cfg(target_arch = "wasm32")]
pub trait StateBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> StateBounds for T {}

pub trait ChannelState: StateBounds {
    /// Expired entries read as absent.
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>>;

    /// `expected = None` requires absence; `replacement = None` deletes.
    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>>;

    fn limits(&self) -> CapabilityLimits;
}

/// A binding without state: every read is absent and every write is
/// `Unsupported`. Hosts that do not persist channel state and tests use it.
#[derive(Debug, Clone, Copy)]
pub struct NoState {
    pub limits: CapabilityLimits,
}

impl Default for NoState {
    fn default() -> Self {
        Self {
            limits: CapabilityLimits {
                operation_total: std::time::Duration::from_secs(1200),
                stream_idle: std::time::Duration::from_secs(300),
                read_bytes: 0,
                write_bytes: 0,
                ws_frame_bytes: 0,
            },
        }
    }
}

impl ChannelState for NoState {
    fn get<'a>(
        &'a self,
        _key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async { Ok(None) })
    }

    fn compare_exchange<'a>(
        &'a self,
        _key: &'a str,
        _expected: Option<Version>,
        _replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "this host keeps no channel state",
            ))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}
