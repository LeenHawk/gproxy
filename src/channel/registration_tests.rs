use std::sync::Arc;

use bytes::Bytes;

use crate::channel::registration::{CHANNEL_REGISTRATIONS, ChannelRegistration, RegisteredChannel};
use crate::channel::registry::{ChannelRegistry, ChannelRegistryError};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};

struct TestChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for TestChannel {
    fn id(&self) -> &'static str {
        "registration-test"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        Vec::new()
    }

    fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Ok(PreparedRequest::new(http::Request::new(Bytes::new())))
    }
}

fn linked_test_channel() -> RegisteredChannel {
    RegisteredChannel::new(Arc::new(TestChannel))
}

#[linkme::distributed_slice(CHANNEL_REGISTRATIONS)]
static LINKED_TEST_CHANNEL: ChannelRegistration = linked_test_channel;

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

#[test]
fn linked_registrations_are_loaded() {
    let registry = ChannelRegistry::with_builtin_and_linked().unwrap();
    assert!(registry.get("registration-test").is_some());
}
