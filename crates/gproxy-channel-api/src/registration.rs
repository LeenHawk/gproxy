//! Compile-time registration contract for externally linked channel crates.

use std::sync::Arc;

use crate::{Channel, ChannelLogin};

/// One channel implementation and its optional interactive-login adapter.
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

/// Startup constructor collected from an externally linked channel crate.
pub type ChannelRegistration = fn() -> RegisteredChannel;

/// Native compile-time registration slice.
#[cfg(all(not(target_arch = "wasm32"), feature = "external-channels"))]
#[linkme::distributed_slice]
pub static CHANNEL_REGISTRATIONS: [ChannelRegistration];
