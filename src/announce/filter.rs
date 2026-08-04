use semver::{Version, VersionReq};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::model::{Notification, RawNotification, Severity};

pub(super) fn applicable(
    entries: Vec<RawNotification>,
    now_unix: i64,
    version: &Version,
) -> Vec<Notification> {
    entries
        .into_iter()
        .filter_map(|entry| convert(entry, now_unix, version))
        .collect()
}

fn convert(entry: RawNotification, now_unix: i64, version: &Version) -> Option<Notification> {
    let severity = Severity::parse(&entry.severity)?;
    entry.content.contains_key("en").then_some(())?;
    OffsetDateTime::parse(&entry.published_at, &Rfc3339).ok()?;
    if let Some(expires_at) = entry.expires_at.as_deref() {
        let expiry = OffsetDateTime::parse(expires_at, &Rfc3339)
            .ok()?
            .unix_timestamp();
        if expiry < now_unix {
            return None;
        }
    }
    if let Some(affects) = entry.affects.as_deref() {
        let requirement = VersionReq::parse(affects).ok()?;
        if !requirement.matches(version) {
            return None;
        }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::announce::model::LocalizedContent;

    fn entry(id: &str) -> RawNotification {
        RawNotification {
            id: id.into(),
            severity: "info".into(),
            published_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            affects: None,
            content: HashMap::from([(
                "en".into(),
                LocalizedContent {
                    title: "Title".into(),
                    body: "Body".into(),
                },
            )]),
        }
    }

    #[test]
    fn filters_expired_mismatched_and_unknown_entries() {
        let mut expired = entry("expired");
        expired.expires_at = Some("2026-01-01T00:00:01Z".into());
        let mut mismatch = entry("mismatch");
        mismatch.affects = Some(">=3.0.0".into());
        let mut unknown = entry("unknown");
        unknown.severity = "future-severity".into();

        let result = applicable(
            vec![entry("active"), expired, mismatch, unknown],
            1_767_225_602,
            &Version::parse("2.2.5").unwrap(),
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "active");
    }
}
