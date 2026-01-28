use std::sync::{Arc, RwLock};

use axum::routing::any;
use axum::Router;
use gproxy_provider_core::Provider;

use crate::auth::AuthProvider;
use crate::handler::proxy_handler;

pub type ProviderLookup =
    Arc<dyn Fn(&str) -> Option<Arc<dyn Provider>> + Send + Sync>;

pub struct CoreState {
    pub lookup: ProviderLookup,
    pub auth: Arc<dyn AuthProvider>,
    pub proxy: Arc<RwLock<Option<String>>>,
}

pub struct Core {
    state: Arc<CoreState>,
}

impl Core {
    pub fn new(
        lookup: ProviderLookup,
        auth: Arc<dyn AuthProvider>,
        proxy: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            state: Arc::new(CoreState {
                lookup,
                auth,
                proxy,
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
