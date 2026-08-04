//! Signed maintainer announcement feed for native instances.

mod fetch;
mod filter;
mod model;

use semver::Version;

use crate::app::AppState;

pub use model::Notification;

const CACHE_TTL_SECONDS: i64 = 6 * 60 * 60;

#[derive(Default)]
pub struct NotificationCache {
    cached: Option<CachedNotifications>,
}

struct CachedNotifications {
    fetched_at: i64,
    notifications: Vec<Notification>,
}

pub async fn list(state: &AppState) -> Vec<Notification> {
    let now = crate::util::time::unix_now();
    if let Some(notifications) = cached(state, now) {
        return notifications;
    }
    let notifications = match state.upstream_client_for_default_proxy() {
        Ok(client) => match Version::parse(env!("CARGO_PKG_VERSION")) {
            Ok(version) => fetch::fetch(client.as_ref(), now, &version).await,
            Err(error) => {
                tracing::warn!(%error, "current version is invalid; announcements unavailable");
                Vec::new()
            }
        },
        Err(error) => {
            tracing::debug!(%error, "announcement client initialization failed");
            Vec::new()
        }
    };
    let mut cache = state
        .notification_cache
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cache.cached = Some(CachedNotifications {
        fetched_at: now,
        notifications: notifications.clone(),
    });
    notifications
}

fn cached(state: &AppState, now: i64) -> Option<Vec<Notification>> {
    let cache = state
        .notification_cache
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    cache.cached.as_ref().and_then(|cached| {
        let age = now.saturating_sub(cached.fetched_at);
        (now >= cached.fetched_at && age < CACHE_TTL_SECONDS).then(|| cached.notifications.clone())
    })
}
