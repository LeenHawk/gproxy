//! Stable-ID lookup of channel implementations. The host registers the channels
//! compiled into its build; a provider naming an unregistered channel is a
//! configuration error, never an implicit fallback to another channel.

use std::{collections::HashMap, sync::Arc};

use super::BaseChannel;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("channel `{0}` is already registered")]
    Duplicate(String),
}

#[derive(Default)]
pub struct ChannelRegistry {
    channels: HashMap<&'static str, Arc<dyn BaseChannel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, channel: Arc<dyn BaseChannel>) -> Result<(), RegistryError> {
        let id = channel.id();
        if self.channels.contains_key(id) {
            return Err(RegistryError::Duplicate(id.to_owned()));
        }
        self.channels.insert(id, channel);
        Ok(())
    }

    pub fn with(mut self, channel: Arc<dyn BaseChannel>) -> Result<Self, RegistryError> {
        self.register(channel)?;
        Ok(self)
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn BaseChannel>> {
        self.channels.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.channels.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}
