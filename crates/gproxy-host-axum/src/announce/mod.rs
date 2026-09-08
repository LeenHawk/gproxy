mod fetch;
mod filter;
mod model;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use bytes::Bytes;
use http::{HeaderValue, Method, Response, StatusCode};
use semver::Version;

use model::Notification;

const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone)]
pub(crate) struct Announcements {
    client: Arc<crate::outbound::OutboundClient>,
    cached: Arc<tokio::sync::Mutex<Option<Cached>>>,
}

struct Cached {
    proxy: crate::outbound::ProxySettings,
    fetched_at: Instant,
    notifications: Vec<Notification>,
}

impl Announcements {
    pub(crate) fn new() -> Self {
        Self {
            client: Arc::new(crate::outbound::OutboundClient::new(concat!(
                "gproxy-announcements/",
                env!("CARGO_PKG_VERSION")
            ))),
            cached: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub(crate) async fn serve(
        &self,
        method: &Method,
        settings: &gproxy_admin::dto::RuntimeSettingsDto,
    ) -> Response<Bytes> {
        if method != Method::GET && method != Method::HEAD {
            return response(StatusCode::METHOD_NOT_ALLOWED, Bytes::new());
        }
        let notifications = self.list(settings).await;
        let value = serde_json::to_string(&notifications).unwrap_or_else(|_| "[]".into());
        let body = if method == Method::HEAD {
            Bytes::new()
        } else {
            Bytes::from(format!("globalThis.__GPROXY_ANNOUNCEMENTS__ = {value};\n"))
        };
        response(StatusCode::OK, body)
    }

    async fn list(&self, settings: &gproxy_admin::dto::RuntimeSettingsDto) -> Vec<Notification> {
        let proxy = crate::outbound::ProxySettings::from(settings);
        let mut cache = self.cached.lock().await;
        if let Some(cached) = cache.as_ref()
            && cached.fetched_at.elapsed() < CACHE_TTL
            && cached.proxy == proxy
        {
            return cached.notifications.clone();
        }
        let notifications = self.fetch_verified(settings).await;
        *cache = Some(Cached {
            proxy,
            fetched_at: Instant::now(),
            notifications: notifications.clone(),
        });
        notifications
    }

    async fn fetch_verified(
        &self,
        settings: &gproxy_admin::dto::RuntimeSettingsDto,
    ) -> Vec<Notification> {
        let Ok(client) = self.client.get(settings) else {
            return Vec::new();
        };
        let Some((bytes, signature)) = fetch::fetch(&client).await else {
            return Vec::new();
        };
        let Some(feed) = fetch::verified(&bytes, &signature) else {
            tracing::warn!("announcement feed verification failed; ignoring feed");
            return Vec::new();
        };
        let Ok(version) = Version::parse(crate::BUILD_VERSION.trim_start_matches('v')) else {
            return Vec::new();
        };
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64);
        filter::applicable(feed.notifications, now, &version, crate::BUILD_CHANNEL)
    }
}

fn response(status: StatusCode, body: Bytes) -> Response<Bytes> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use ed25519_dalek::{Signer as _, SigningKey};

    #[test]
    fn tampered_feed_is_rejected_before_parsing() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let feed = br#"{"version":1,"notifications":[]}"#;
        let signature = base64::engine::general_purpose::STANDARD.encode(key.sign(feed).to_bytes());
        let public_key =
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());

        assert!(super::fetch::verified_with_key(feed, &signature, &public_key).is_some());
        assert!(
            super::fetch::verified_with_key(b"{\"version\":2}", &signature, &public_key).is_none()
        );
    }
}
