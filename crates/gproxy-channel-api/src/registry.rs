//! The channel registry: id → adapter, fixed at startup.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::channel::Channel;
use crate::login::ChannelLoginRef;

/// Built once at startup from the built-in set plus any compile-time
/// linked extensions (the native `linkme` collection lives with the app,
/// not here). Duplicate ids fail construction — v2 policy, kept.
#[derive(Clone)]
pub struct ChannelRegistry {
    channels: BTreeMap<&'static str, Arc<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new(
        channels: impl IntoIterator<Item = Box<dyn Channel>>,
    ) -> Result<Self, DuplicateChannel> {
        let mut map = BTreeMap::new();
        for channel in channels {
            let id = channel.descriptor().id;
            if map.insert(id, Arc::from(channel)).is_some() {
                return Err(DuplicateChannel(id));
            }
        }
        Ok(Self { channels: map })
    }

    pub fn get(&self, id: &str) -> Option<&dyn Channel> {
        self.channels.get(id).map(AsRef::as_ref)
    }

    pub fn shared(&self, id: &str) -> Option<Arc<dyn Channel>> {
        self.channels.get(id).cloned()
    }

    pub fn login_for(&self, id: &str) -> Option<ChannelLoginRef<'_>> {
        self.get(id)?.login()
    }

    /// Runtime catalog for the console and admin API.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Channel> {
        self.channels.values().map(AsRef::as_ref)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("duplicate channel id: {0}")]
pub struct DuplicateChannel(pub &'static str);
