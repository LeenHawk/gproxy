use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
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
    pub(super) lease_calls: usize,
    pub(super) rotations: Vec<u64>,
    pub(super) authorizations: Vec<String>,
    pub(super) settlements: Vec<Settlement>,
    pub(super) captures: Vec<(Option<StatusCode>, Option<Bytes>)>,
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
                lease_calls: 0,
                rotations: Vec::new(),
                authorizations: Vec::new(),
                settlements: Vec::new(),
                captures: Vec::new(),
            })),
        }
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
}

impl ControlPlane for MemoryHost {
    fn resolve(&self, _: Option<&str>, _: &RoutingMode) -> Result<Plan, CoreError> {
        Err(CoreError::UnknownRoute("unused".into()))
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
