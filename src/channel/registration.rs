//! Compile-time registration contract for externally linked channel crates.

use std::sync::Arc;

use super::{Channel, ChannelLogin};

/// One channel implementation and its optional interactive-login adapter.
///
/// Constructors in [`CHANNEL_REGISTRATIONS`] return this bundle at startup. A
/// shared concrete `Arc<T>` may be coerced into both trait objects when the
/// channel and login implementation carry common state.
pub struct RegisteredChannel {
    pub channel: Arc<dyn Channel>,
    pub login: Option<Arc<dyn ChannelLogin>>,
}

impl RegisteredChannel {
    pub fn new(channel: Arc<dyn Channel>) -> Self {
        Self {
            channel,
            login: None,
        }
    }

    pub fn with_login(channel: Arc<dyn Channel>, login: Arc<dyn ChannelLogin>) -> Self {
        Self {
            channel,
            login: Some(login),
        }
    }
}

/// Constructor collected from an externally linked channel crate.
pub type ChannelRegistration = fn() -> RegisteredChannel;

/// Native compile-time registration slice.
///
/// External crates add a constructor with `linkme::distributed_slice`. The
/// final binary must reference the crate (usually `use crate_name as _;`) so its
/// registration object is retained by the linker.
#[cfg(all(not(target_arch = "wasm32"), feature = "external-channels"))]
#[linkme::distributed_slice]
pub static CHANNEL_REGISTRATIONS: [ChannelRegistration];

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::channel::registry::{ChannelRegistry, ChannelRegistryError};
    use crate::channel::{ChannelError, PrepareCtx, PreparedRequest};
    use crate::protocol::Provider;

    struct TestChannel;

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl Channel for TestChannel {
        fn id(&self) -> &'static str {
            "registration-test"
        }

        fn provider_family(&self) -> Provider {
            Provider::OpenAi
        }

        fn routing_table(&self) -> crate::channel::routes::RouteList {
            Vec::new()
        }

        fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
            Ok(PreparedRequest::new(http::Request::new(Bytes::new())))
        }
    }

    #[test]
    fn duplicate_channel_ids_are_rejected() {
        let mut registry = ChannelRegistry::with_builtin();
        registry
            .register(RegisteredChannel::new(Arc::new(TestChannel)))
            .unwrap();

        assert_eq!(
            registry
                .register(RegisteredChannel::new(Arc::new(TestChannel)))
                .unwrap_err(),
            ChannelRegistryError::DuplicateChannel("registration-test")
        );
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "external-channels"))]
    fn linked_test_channel() -> RegisteredChannel {
        RegisteredChannel::new(Arc::new(TestChannel))
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "external-channels"))]
    #[linkme::distributed_slice(CHANNEL_REGISTRATIONS)]
    static LINKED_TEST_CHANNEL: ChannelRegistration = linked_test_channel;

    #[cfg(all(not(target_arch = "wasm32"), feature = "external-channels"))]
    #[test]
    fn linked_registrations_are_loaded() {
        let registry = ChannelRegistry::with_builtin_and_linked().unwrap();
        assert!(registry.get("registration-test").is_some());
    }
}
