//! A scripted `ServiceCaller` for the vendor service tests: in-memory
//! bindings, a fixed identity and usage, a configurable role.
#![allow(dead_code)]

use gproxy_channel::channel::{
    CallerIdentity, CallerRole, CallerUsage, OperationFuture, ResourceBindingRecord, ServiceCaller,
};
use std::sync::Mutex;

pub struct ScriptCaller {
    pub role: CallerRole,
    pub identity: CallerIdentity,
    pub usage: CallerUsage,
    pub bindings: Mutex<Vec<ResourceBindingRecord>>,
}

impl ScriptCaller {
    pub fn new(role: CallerRole, id: &str) -> Self {
        Self {
            role,
            identity: CallerIdentity {
                id: id.to_owned(),
                display_name: None,
            },
            usage: CallerUsage::default(),
            bindings: Mutex::new(Vec::new()),
        }
    }
    pub fn member(id: &str) -> Self {
        Self::new(CallerRole::Member, id)
    }
    pub fn admin(id: &str) -> Self {
        Self::new(CallerRole::Admin, id)
    }
    pub fn with_usage(mut self, usage: CallerUsage) -> Self {
        self.usage = usage;
        self
    }
    pub fn with_binding(
        self,
        kind: &str,
        upstream_id: &str,
        credential_id: &str,
        summary: serde_json::Value,
    ) -> Self {
        self.bindings.lock().unwrap().push(ResourceBindingRecord {
            kind: kind.to_owned(),
            upstream_id: upstream_id.to_owned(),
            credential_id: credential_id.to_owned(),
            summary,
        });
        self
    }
    pub fn bound(&self) -> Vec<ResourceBindingRecord> {
        self.bindings.lock().unwrap().clone()
    }
}

impl ServiceCaller for ScriptCaller {
    fn role(&self) -> CallerRole {
        self.role
    }
    fn identity(&self) -> &CallerIdentity {
        &self.identity
    }
    fn usage<'a>(&'a self) -> OperationFuture<'a, CallerUsage> {
        let usage = self.usage.clone();
        Box::pin(async move { Ok(usage) })
    }
    fn find_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, Option<ResourceBindingRecord>> {
        let found = self
            .bindings
            .lock()
            .unwrap()
            .iter()
            .find(|b| b.kind == kind && b.upstream_id == upstream_id)
            .cloned();
        Box::pin(async move { Ok(found) })
    }
    fn list_bindings<'a>(
        &'a self,
        kind: &'a str,
    ) -> OperationFuture<'a, Vec<ResourceBindingRecord>> {
        let found = self
            .bindings
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.kind == kind)
            .cloned()
            .collect();
        Box::pin(async move { Ok(found) })
    }
    fn save_binding<'a>(&'a self, record: ResourceBindingRecord) -> OperationFuture<'a, ()> {
        self.bindings.lock().unwrap().push(record);
        Box::pin(async { Ok(()) })
    }
    fn delete_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, ()> {
        self.bindings
            .lock()
            .unwrap()
            .retain(|b| !(b.kind == kind && b.upstream_id == upstream_id));
        Box::pin(async { Ok(()) })
    }
}
