use bytes::Bytes;
use semver::Version;

use crate::http::client::UpstreamClient;
use crate::selfupdate::verify::verify_detached;
use crate::site::SITE_BASE_URL;

use super::filter::applicable;
use super::model::{Feed, Notification};

const FEED_URL: &str = "/notifications.json";
const SIGNATURE_URL: &str = "/notifications.json.sig";

pub(super) async fn fetch(
    client: &dyn UpstreamClient,
    now_unix: i64,
    version: &Version,
) -> Vec<Notification> {
    let Some(bytes) = get(client, FEED_URL).await else {
        return Vec::new();
    };
    let Some(signature) = get(client, SIGNATURE_URL).await else {
        tracing::warn!("announcement signature is unavailable; ignoring feed");
        return Vec::new();
    };
    let signature = match std::str::from_utf8(&signature) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "announcement signature is not UTF-8; ignoring feed");
            return Vec::new();
        }
    };
    if let Err(error) = verify_detached(&bytes, signature) {
        tracing::warn!(%error, "announcement signature verification failed; ignoring feed");
        return Vec::new();
    }
    let feed = match serde_json::from_slice::<Feed>(&bytes) {
        Ok(feed) => feed,
        Err(error) => {
            tracing::warn!(%error, "announcement feed is invalid; ignoring feed");
            return Vec::new();
        }
    };
    if feed.version != 1 {
        tracing::warn!(
            version = feed.version,
            "unsupported announcement feed version"
        );
        return Vec::new();
    }
    applicable(feed.notifications, now_unix, version)
}

async fn get(client: &dyn UpstreamClient, path: &str) -> Option<Bytes> {
    let url = format!("{SITE_BASE_URL}{path}");
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header(http::header::USER_AGENT, "gproxy-announcements")
        .body(Bytes::new())
        .ok()?;
    let response = match client.send(request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(%error, %url, "failed to fetch announcement resource");
            return None;
        }
    };
    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), %url, "announcement request was unsuccessful");
        return None;
    }
    Some(response.into_body())
}
