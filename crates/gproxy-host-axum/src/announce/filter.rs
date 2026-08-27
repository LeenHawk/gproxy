use semver::{Version, VersionReq};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::model::{Notification, RawNotification, Severity};

pub(super) fn applicable(
    entries: Vec<RawNotification>,
    now_unix: i64,
    version: &Version,
    channel: &str,
) -> Vec<Notification> {
    entries
        .into_iter()
        .filter_map(|entry| convert(entry, now_unix, version, channel))
        .collect()
}

fn convert(
    entry: RawNotification,
    now_unix: i64,
    version: &Version,
    channel: &str,
) -> Option<Notification> {
    let severity = Severity::parse(&entry.severity)?;
    entry.content.contains_key("en").then_some(())?;
    OffsetDateTime::parse(&entry.published_at, &Rfc3339).ok()?;
    if let Some(expires_at) = entry.expires_at.as_deref()
        && OffsetDateTime::parse(expires_at, &Rfc3339)
            .ok()?
            .unix_timestamp()
            < now_unix
    {
        return None;
    }
    if let Some(affects) = entry.affects.as_deref()
        && !VersionReq::parse(affects).ok()?.matches(version)
    {
        return None;
    }
    if !entry.channels.is_empty() && !entry.channels.iter().any(|value| value == channel) {
        return None;
    }
    Some(Notification {
        id: entry.id,
        severity,
        published_at: entry.published_at,
        expires_at: entry.expires_at,
        affects: entry.affects,
        content: entry.content,
    })
}
