use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use gproxy_channel_api::{Binding, BindingStore, BoxFuture, CallerIdentity, UsageView};
use gproxy_protocol::OperationKey;
use http::StatusCode;
use rust_decimal::Decimal;
use serde_json::json;

use crate::boundary::RoutingMode;
use crate::control::{ControlPlane, Plan, Pricing, ProviderRef};
use crate::error::CoreError;
use crate::host::{CredentialId, CredentialRecord, Host};
use crate::usage::Settlement;

#[derive(Clone)]
pub(super) struct MemoryHost {
    pub(super) state: Arc<Mutex<State>>,
}

pub(super) struct State {
    pub(super) credential: CredentialRecord,
    pub(super) conflict: bool,
    pub(super) peer_refresh_on_wait: bool,
    pub(super) lease_calls: usize,
    pub(super) wait_calls: usize,
    pub(super) rotations: Vec<u64>,
    pub(super) authorizations: Vec<String>,
    pub(super) loaded_credentials: Vec<CredentialId>,
    pub(super) settlements: Vec<Settlement>,
    pub(super) captures: Vec<Captured>,
    pub(super) auth_calls: usize,
    pub(super) admit_calls: usize,
    pub(super) plan: Option<Plan>,
    pub(super) statuses: VecDeque<StatusCode>,
    pub(super) resolved_models: Vec<Option<String>>,
    pub(super) admission_finishes: Vec<bool>,
    pub(super) bindings_enabled: bool,
    pub(super) bindings: BTreeMap<(i64, i64, String, String), Binding>,
    pub(super) cache: BTreeMap<String, Vec<u8>>,
    pub(super) cache_ttls: BTreeMap<String, u64>,
    pub(super) caller_user_id: i64,
    pub(super) caller_key_id: i64,
    pub(super) socket_opens: usize,
    pub(super) socket_closed: bool,
    pub(super) omit_usage: bool,
}

pub(super) struct Captured {
    pub(super) status: Option<StatusCode>,
    pub(super) body: Option<Bytes>,
    pub(super) provider_id: Option<i64>,
    pub(super) credential_id: Option<CredentialId>,
}

impl MemoryHost {
    pub(super) fn new(conflict: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                credential: CredentialRecord {
                    id: CredentialId(7),
                    channel: "memory".into(),
                    secret: json!({"access_token": "old", "expires_at": 0}),
                    version: 4,
                },
                conflict,
                peer_refresh_on_wait: false,
                lease_calls: 0,
                wait_calls: 0,
                rotations: Vec::new(),
                authorizations: Vec::new(),
                loaded_credentials: Vec::new(),
                settlements: Vec::new(),
                captures: Vec::new(),
                auth_calls: 0,
                admit_calls: 0,
                plan: None,
                statuses: VecDeque::new(),
                resolved_models: Vec::new(),
                admission_finishes: Vec::new(),
                bindings_enabled: true,
                bindings: BTreeMap::new(),
                cache: BTreeMap::new(),
                cache_ttls: BTreeMap::new(),
                caller_user_id: 1,
                caller_key_id: 2,
                socket_opens: 0,
                socket_closed: false,
                omit_usage: false,
            })),
        }
    }

    pub(super) fn without_bindings() -> Self {
        let host = Self::new(false);
        host.state.lock().expect("state lock").bindings_enabled = false;
        host
    }
}

impl Host for MemoryHost {
    type Credentials = Self;
    type Cache = Self;
    type Transport = Self;
    type Usage = Self;
    type Capture = Self;

    fn credentials(&self) -> &Self::Credentials {
        self
    }
    fn cache(&self) -> &Self::Cache {
        self
    }
    fn transport(&self) -> &Self::Transport {
        self
    }
    fn usage(&self) -> &Self::Usage {
        self
    }
    fn capture(&self) -> &Self::Capture {
        self
    }
    fn authenticate<'a>(
        &'a self,
        _: &'a crate::boundary::RequestCtx,
    ) -> BoxFuture<'a, Result<CallerIdentity, CoreError>> {
        let mut state = self.state.lock().expect("state lock");
        state.auth_calls += 1;
        let identity = CallerIdentity {
            user_id: state.caller_user_id,
            user_key_id: state.caller_key_id,
            org_id: None,
            team_id: None,
        };
        Box::pin(async move { Ok(identity) })
    }
    fn admit<'a>(
        &'a self,
        _: &'a CallerIdentity,
        _: &'a crate::boundary::RequestCtx,
        _: Option<OperationKey>,
        _: &'a Plan,
    ) -> BoxFuture<'a, Result<(), CoreError>> {
        self.state.lock().expect("state lock").admit_calls += 1;
        Box::pin(async { Ok(()) })
    }
    fn finish_admission<'a>(
        &'a self,
        _: &'a str,
        settlement: Option<&'a Settlement>,
    ) -> BoxFuture<'a, ()> {
        self.state
            .lock()
            .expect("state lock")
            .admission_finishes
            .push(settlement.is_some());
        Box::pin(async {})
    }
    fn wait<'a>(&'a self, _: std::time::Duration) -> BoxFuture<'a, ()> {
        let mut state = self.state.lock().expect("state lock");
        state.wait_calls += 1;
        if state.peer_refresh_on_wait {
            state.peer_refresh_on_wait = false;
            state.credential.secret = json!({
                "access_token": "peer",
                "expires_at": i64::MAX
            });
            state.credential.version += 1;
        }
        Box::pin(async {})
    }
    fn surface_usage<'a>(
        &'a self,
        _: &'a CallerIdentity,
        _: &'a ProviderRef,
    ) -> Box<dyn UsageView + 'a> {
        Box::new(self.clone())
    }
    fn bindings(&self) -> Option<&dyn BindingStore> {
        self.state
            .lock()
            .expect("state lock")
            .bindings_enabled
            .then_some(self as &dyn BindingStore)
    }
}

impl ControlPlane for MemoryHost {
    fn resolve(&self, model: Option<&str>, _: &RoutingMode) -> Result<Plan, CoreError> {
        let mut state = self.state.lock().expect("state lock");
        state.resolved_models.push(model.map(str::to_owned));
        state
            .plan
            .clone()
            .ok_or_else(|| CoreError::UnknownRoute("unused".into()))
    }

    fn pricing(&self, _: &ProviderRef, _: &str) -> Option<Pricing> {
        Some(Pricing {
            input_per_million: Decimal::ONE,
            output_per_million: Decimal::from(2),
            cached_input_per_million: None,
            metric_rates: BTreeMap::new(),
        })
    }
}
