//! Stable-ID lookup of upstream kinds. The host registers the kinds compiled
//! into its build; a provider naming an unregistered kind is a configuration
//! error, never an implicit fallback to another kind.

use std::{collections::HashMap, sync::Arc};

use crate::UpstreamKind;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("channel `{0}` is already registered")]
    Duplicate(String),
}

/// `K` is the trait object the host works with: `dyn UpstreamKind`, or a
/// trait that builds on it.
pub struct UpstreamRegistry<K: ?Sized = dyn UpstreamKind> {
    kinds: HashMap<&'static str, Arc<K>>,
}

impl<K: ?Sized> Default for UpstreamRegistry<K> {
    fn default() -> Self {
        Self {
            kinds: HashMap::new(),
        }
    }
}

impl<K: UpstreamKind + ?Sized> UpstreamRegistry<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, kind: Arc<K>) -> Result<(), RegistryError> {
        let id = kind.id();
        if self.kinds.contains_key(id) {
            return Err(RegistryError::Duplicate(id.to_owned()));
        }
        self.kinds.insert(id, kind);
        Ok(())
    }

    pub fn with(mut self, kind: Arc<K>) -> Result<Self, RegistryError> {
        self.register(kind)?;
        Ok(self)
    }

    pub fn get(&self, id: &str) -> Option<&Arc<K>> {
        self.kinds.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.kinds.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}
