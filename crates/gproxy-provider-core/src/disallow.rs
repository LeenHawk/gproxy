use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DisallowScope {
    AllModels,
    Model(String),
}

impl DisallowScope {
    pub fn model<S: Into<String>>(model: S) -> Self {
        Self::Model(model.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisallowLevel {
    Cooldown,
    Transient,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisallowKey {
    pub credential_id: String,
    pub scope: DisallowScope,
}

impl DisallowKey {
    pub fn new(credential_id: impl Into<String>, scope: DisallowScope) -> Self {
        Self {
            credential_id: credential_id.into(),
            scope,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DisallowEntry {
    pub level: DisallowLevel,
    pub until: Option<SystemTime>,
    pub reason: Option<String>,
    pub updated_at: SystemTime,
}

impl DisallowEntry {
    pub fn is_active(&self, now: SystemTime) -> bool {
        match self.until {
            Some(until) => until > now,
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DisallowMark {
    pub scope: DisallowScope,
    pub level: DisallowLevel,
    pub duration: Option<Duration>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DisallowRecord {
    pub provider: String,
    pub credential_id: String,
    pub scope: DisallowScope,
    pub level: DisallowLevel,
    pub until: Option<SystemTime>,
    pub reason: Option<String>,
    pub updated_at: SystemTime,
}
