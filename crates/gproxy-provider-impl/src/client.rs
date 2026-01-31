use std::sync::{Arc, OnceLock};

use gproxy_provider_core::{AttemptFailure, UpstreamPassthroughError};
use wreq::Proxy;

struct SharedClient {
    proxy: Option<String>,
    client: Arc<wreq::Client>,
}

static SHARED_CLIENT: OnceLock<SharedClient> = OnceLock::new();

#[allow(clippy::result_large_err)]
pub(crate) fn shared_client(proxy: Option<&str>) -> Result<Arc<wreq::Client>, AttemptFailure> {
    let proxy_owned = proxy.map(|value| value.to_string());
    if let Some(shared) = SHARED_CLIENT.get() {
        if shared.proxy != proxy_owned {
            return Err(AttemptFailure {
                passthrough: UpstreamPassthroughError::service_unavailable(
                    "proxy mismatch: only a single global proxy is supported".to_string(),
                ),
                mark: None,
            });
        }
        return Ok(shared.client.clone());
    }

    let mut builder = wreq::Client::builder();
    if let Some(proxy_url) = proxy {
        let proxy = Proxy::all(proxy_url)
            .map_err(|err| AttemptFailure {
                passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                mark: None,
            })?;
        builder = builder.proxy(proxy);
    }

    let client = builder.build().map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let shared = SharedClient {
        proxy: proxy_owned,
        client: Arc::new(client),
    };
    let _ = SHARED_CLIENT.set(shared);
    Ok(SHARED_CLIENT
        .get()
        .expect("shared client must be set")
        .client
        .clone())
}
