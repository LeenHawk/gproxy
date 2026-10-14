//! The publisher's announcement feed: fetched from the documentation site,
//! verified under the update signing key, and reduced to the notices that
//! apply to this build.
//!
//! # Why it lives next to the updater
//!
//! The feed is signed by the same Ed25519 identity that signs every update
//! manifest (`.github/workflows/ci.yml` signs `notifications.json` on deploy
//! with `UPDATE_SIGNING_PRIVATE_KEY_B64`), so the trust root is the one
//! [`super::config::SIGNING_PUBLIC_KEY`] already compiles in. A notice that
//! says "upgrade now, 4.0.3 leaks credentials" is exactly the kind of text a
//! mirror or a captive portal must not be able to forge, which is why
//! verification here is **not** subject to the operator's
//! `verify_signature` switch: that switch trusts a *source they chose*, and
//! nobody chose this feed.
//!
//! # Failure is silence
//!
//! Every function returns `Option` or an empty list. The feed is a courtesy,
//! and an updater that reported an error because a documentation site was
//! down would be inventing an outage. A feed that fails verification is
//! logged at `warn` and then ignored, because that one *is* worth an
//! operator's attention.
//!
//! # Filtering happens here, not in the console
//!
//! `affects` is a semver range and `channels` a list of update channels; both
//! are matched against this process rather than shipped raw, so a console
//! never has to carry a semver parser and a notice for `<4.0.2` never reaches
//! an instance on 4.0.4.

use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use gproxy_host_axum::{Announcement, AnnouncementContent, AnnouncementSeverity};
use semver::{Version, VersionReq};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::signature;

const FEED_URL: &str = "https://gproxy.leenhawk.com/notifications.json";
const SIGNATURE_URL: &str = "https://gproxy.leenhawk.com/notifications.json.sig";

/// How long one fetch is served from memory. A console polls the page; the
/// documentation site should see one request per instance per quarter day.
const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// After a failed fetch, how long before trying again. Shorter than the TTL,
/// because the failure may have been a flaky link; long enough that a dead
/// site is not hammered by every page load.
const RETRY_AFTER: Duration = Duration::from_secs(15 * 60);

/// A feed is a few notices, each a few paragraphs. Anything larger is not the
/// feed this process knows how to read.
const MAX_BYTES: usize = 256 * 1024;

/// The in-memory copy, with when it was taken.
#[derive(Default)]
pub(super) struct Cache(Mutex<Option<Entry>>);

struct Entry {
    fetched_at: Instant,
    ttl: Duration,
    notices: Vec<Announcement>,
}

impl Cache {
    /// The cached list when it is fresh, else `None`.
    fn fresh(&self) -> Option<Vec<Announcement>> {
        let guard = self.0.lock().unwrap_or_else(|error| error.into_inner());
        guard
            .as_ref()
            .filter(|entry| entry.fetched_at.elapsed() < entry.ttl)
            .map(|entry| entry.notices.clone())
    }

    fn store(&self, notices: Vec<Announcement>, ttl: Duration) {
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(Entry {
            fetched_at: Instant::now(),
            ttl,
            notices,
        });
    }
}

/// The notices that apply to `version` on `channel`, from cache or the wire.
pub(super) async fn applicable(
    cache: &Cache,
    client: &reqwest::Client,
    key: Option<&str>,
    version: &str,
    channel: &str,
) -> Vec<Announcement> {
    if let Some(notices) = cache.fresh() {
        return notices;
    }
    let Some(feed) = fetch(client, key).await else {
        cache.store(Vec::new(), RETRY_AFTER);
        return Vec::new();
    };
    let now = OffsetDateTime::now_utc();
    let notices = filter(feed.notifications, now, version, channel);
    cache.store(notices.clone(), CACHE_TTL);
    notices
}

async fn fetch(client: &reqwest::Client, key: Option<&str>) -> Option<Feed> {
    let bytes = get(client, FEED_URL).await?;
    let signature = get(client, SIGNATURE_URL).await?;
    let signature = String::from_utf8(signature).ok()?;
    let feed = verified(&bytes, &signature, key);
    if feed.is_none() {
        tracing::warn!("announcement feed failed signature verification; ignoring it");
    }
    feed
}

async fn get(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    (bytes.len() <= MAX_BYTES).then(|| bytes.to_vec())
}

/// Verify first, parse second: a body that does not verify is never handed to
/// a JSON parser, however well-formed it looks.
pub(super) fn verified(bytes: &[u8], signature: &str, key: Option<&str>) -> Option<Feed> {
    signature::verify_detached_with_key(bytes, signature, key).ok()?;
    let feed = serde_json::from_slice::<Feed>(bytes).ok()?;
    (feed.version == 1).then_some(feed)
}

/// Drop what has expired, what targets another version, what targets another
/// channel, and what is malformed; keep the rest in published order.
pub(super) fn filter(
    entries: Vec<RawNotification>,
    now: OffsetDateTime,
    version: &str,
    channel: &str,
) -> Vec<Announcement> {
    let version = Version::parse(version.trim_start_matches('v')).ok();
    entries
        .into_iter()
        .filter_map(|entry| convert(entry, now, version.as_ref(), channel))
        .collect()
}

fn convert(
    entry: RawNotification,
    now: OffsetDateTime,
    version: Option<&Version>,
    channel: &str,
) -> Option<Announcement> {
    let severity = match entry.severity.as_str() {
        "info" => AnnouncementSeverity::Info,
        "warning" => AnnouncementSeverity::Warning,
        "critical" => AnnouncementSeverity::Critical,
        _ => return None,
    };
    entry.content.contains_key("en").then_some(())?;
    OffsetDateTime::parse(&entry.published_at, &Rfc3339).ok()?;
    if let Some(expires_at) = entry.expires_at.as_deref()
        && OffsetDateTime::parse(expires_at, &Rfc3339).ok()? < now
    {
        return None;
    }
    if let Some(affects) = entry.affects.as_deref() {
        // A build whose version does not parse cannot be shown to match a
        // range, so a targeted notice is withheld rather than guessed at.
        let version = version?;
        if !VersionReq::parse(affects).ok()?.matches(version) {
            return None;
        }
    }
    if !entry.channels.is_empty() && !entry.channels.iter().any(|value| value == channel) {
        return None;
    }
    Some(Announcement {
        id: entry.id,
        severity,
        published_at: entry.published_at,
        expires_at: entry.expires_at,
        affects: entry.affects,
        content: entry
            .content
            .into_iter()
            .map(|(locale, content)| {
                (
                    locale,
                    AnnouncementContent {
                        title: content.title,
                        body: content.body,
                    },
                )
            })
            .collect(),
    })
}

/// The wire shape, as `docs/scripts/check-notifications.mjs` validates it.
#[derive(Deserialize)]
pub(super) struct Feed {
    pub version: u32,
    pub notifications: Vec<RawNotification>,
}

#[derive(Deserialize)]
pub(super) struct RawNotification {
    pub id: String,
    pub severity: String,
    pub published_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub affects: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    pub content: BTreeMap<String, RawContent>,
}

#[derive(Deserialize)]
pub(super) struct RawContent {
    pub title: String,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use ed25519_dalek::{Signer as _, SigningKey};
    use time::macros::datetime;

    use super::*;

    fn encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn notice(id: &str, extra: &str) -> String {
        format!(
            r#"{{"id":"{id}","severity":"warning","published_at":"2026-10-01T00:00:00Z",{extra}"content":{{"en":{{"title":"T","body":"B"}}}}}}"#
        )
    }

    fn feed(entries: &[String]) -> Vec<RawNotification> {
        let json = format!(r#"{{"version":1,"notifications":[{}]}}"#, entries.join(","));
        serde_json::from_str::<Feed>(&json).unwrap().notifications
    }

    #[test]
    fn a_tampered_feed_is_rejected_before_it_is_parsed() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = encode(key.verifying_key().as_bytes());
        let body = br#"{"version":1,"notifications":[]}"#;
        let signature = encode(&key.sign(body).to_bytes());

        assert!(verified(body, &signature, Some(&public)).is_some());
        assert!(
            verified(
                br#"{"version":1,"notifications":[{}]}"#,
                &signature,
                Some(&public)
            )
            .is_none()
        );
        assert!(
            verified(body, &signature, None).is_none(),
            "no key means no feed"
        );
        // Verified, but not a feed this process reads.
        let v2 = br#"{"version":2,"notifications":[]}"#;
        let signature = encode(&key.sign(v2).to_bytes());
        assert!(verified(v2, &signature, Some(&public)).is_none());
    }

    #[test]
    fn filtering_keeps_only_what_applies_to_this_build() {
        let now = datetime!(2026-10-03 00:00 UTC);
        let entries = feed(&[
            notice("all", ""),
            notice("expired", r#""expires_at":"2026-10-02T00:00:00Z","#),
            notice("future-expiry", r#""expires_at":"2026-12-01T00:00:00Z","#),
            notice("older-only", r#""affects":"<4.0.4","#),
            notice("this-range", r#""affects":">=4.0.0, <5.0.0","#),
            notice("dev-only", r#""channels":["dev"],"#),
            notice("release-too", r#""channels":["dev","release"],"#),
            notice("bad-range", r#""affects":"not a range","#),
            r#"{"id":"no-en","severity":"info","published_at":"2026-10-01T00:00:00Z","content":{"zh-CN":{"title":"T","body":"B"}}}"#.into(),
            r#"{"id":"bad-severity","severity":"loud","published_at":"2026-10-01T00:00:00Z","content":{"en":{"title":"T","body":"B"}}}"#.into(),
        ]);
        let ids: Vec<String> = filter(entries, now, "v4.0.4", "release")
            .into_iter()
            .map(|notice| notice.id)
            .collect();
        assert_eq!(ids, ["all", "future-expiry", "this-range", "release-too"]);
    }

    #[test]
    fn an_unparseable_build_version_withholds_targeted_notices_only() {
        let now = datetime!(2026-10-03 00:00 UTC);
        let entries = feed(&[
            notice("all", ""),
            notice("targeted", r#""affects":"^4.0.0","#),
        ]);
        let ids: Vec<String> = filter(entries, now, "nightly-abc123", "dev")
            .into_iter()
            .map(|notice| notice.id)
            .collect();
        assert_eq!(ids, ["all"]);
    }

    #[test]
    fn the_cache_serves_a_fresh_entry_and_forgets_a_stale_one() {
        let cache = Cache::default();
        assert!(cache.fresh().is_none());
        cache.store(Vec::new(), Duration::from_secs(60));
        assert!(cache.fresh().is_some());
        cache.store(Vec::new(), Duration::ZERO);
        assert!(cache.fresh().is_none());
    }
}
