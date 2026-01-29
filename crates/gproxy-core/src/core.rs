use std::sync::{Arc, RwLock};

use axum::routing::any;
use axum::Router;
use gproxy_provider_core::{Provider, SharedTrafficSink, NoopTrafficSink};

use crate::auth::AuthProvider;
use crate::handler::proxy_handler;

pub type ProviderLookup =
    Arc<dyn Fn(&str) -> Option<Arc<dyn Provider>> + Send + Sync>;

pub struct CoreState {
    pub lookup: ProviderLookup,
    pub auth: Arc<dyn AuthProvider>,
    pub proxy: Arc<RwLock<Option<String>>>,
    pub traffic: SharedTrafficSink,
    pub provider_ids: Arc<RwLock<std::collections::HashMap<String, i64>>>,
}

pub struct Core {
    state: Arc<CoreState>,
}

impl Core {
    pub fn new(
        lookup: ProviderLookup,
        auth: Arc<dyn AuthProvider>,
        proxy: Arc<RwLock<Option<String>>>,
        traffic: Option<SharedTrafficSink>,
        provider_ids: Option<std::collections::HashMap<String, i64>>,
    ) -> Self {
        Self {
            state: Arc::new(CoreState {
                lookup,
                auth,
                proxy,
                traffic: traffic.unwrap_or_else(|| Arc::new(NoopTrafficSink)),
                provider_ids: Arc::new(RwLock::new(provider_ids.unwrap_or_default())),
            }),
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/{provider}/{*path}", any(proxy_handler))
            .with_state(self.state.clone())
    }

    pub fn state(&self) -> Arc<CoreState> {
        self.state.clone()
    }
}
