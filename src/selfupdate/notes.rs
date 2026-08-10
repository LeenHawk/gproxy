//! Cumulative stable release notes fetched on demand from the project site.

use bytes::{Bytes, BytesMut};
use futures_util::{StreamExt, future::join_all};
use semver::Version;
use serde::{Deserialize, Serialize};

use super::{Channel, UpdateContext, UpdateError};
use crate::http::client::RespStream;
use crate::site::SITE_BASE_URL;

const MAX_INDEX_BYTES: usize = 64 * 1024;
const MAX_NOTES_BYTES: usize = 64 * 1024;
const MAX_TOTAL_NOTES_BYTES: usize = 512 * 1024;
const MAX_RELEASES: usize = 256;
const FETCH_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseNotesEntry {
    pub version: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseNotesReport {
    pub current: String,
    pub latest: String,
    /// False when an indexed note could not be fetched or a response bound
    /// prevented the complete interval from being returned.
    pub complete: bool,
    pub entries: Vec<ReleaseNotesEntry>,
}

#[derive(Debug, Deserialize)]
struct ReleaseIndex {
    versions: Vec<String>,
}

/// Return all published stable notes in `(current, latest]`, newest first.
///
/// Staging builds use commit identities rather than SemVer, so release notes
/// intentionally degrade to an empty response there. The index fetch is a hard
/// failure (the Console can offer Retry); individual note failures are partial
/// success and set `complete=false`.
pub async fn fetch_range(
    ctx: &UpdateContext,
    current: &str,
    latest: &str,
) -> Result<ReleaseNotesReport, UpdateError> {
    let mut report = ReleaseNotesReport {
        current: current.to_string(),
        latest: latest.to_string(),
        complete: true,
        entries: Vec::new(),
    };
    if ctx.channel == Channel::Staging || current == latest {
        return Ok(report);
    }

    let index_url = format!("{SITE_BASE_URL}/release-notes/index.json");
    let index_bytes = http_get(ctx, &index_url, MAX_INDEX_BYTES)
        .await
        .map_err(UpdateError::ReleaseNotes)?;
    let index: ReleaseIndex = serde_json::from_slice(&index_bytes).map_err(|error| {
        UpdateError::ReleaseNotes(format!("invalid release notes index: {error}"))
    })?;
    let selection = select_versions(index.versions, current, latest)?;
    report.complete = selection.complete;

    let mut total_bytes = 0usize;
    'batches: for batch in selection.versions.chunks(FETCH_CONCURRENCY) {
        let results = join_all(batch.iter().map(|version| fetch_one(ctx, version))).await;
        for (version, result) in batch.iter().zip(results) {
            match result {
                Ok(body) => {
                    let Some(next_total) = total_bytes.checked_add(body.len()) else {
                        report.complete = false;
                        break 'batches;
                    };
                    if next_total > MAX_TOTAL_NOTES_BYTES {
                        report.complete = false;
                        break 'batches;
                    }
                    total_bytes = next_total;
                    report.entries.push(ReleaseNotesEntry {
                        version: version.to_string(),
                        body,
                    });
                }
                Err(error) => {
                    report.complete = false;
                    tracing::debug!(%version, %error, "failed to fetch indexed release notes");
                }
            }
        }
    }

    Ok(report)
}

struct VersionSelection {
    versions: Vec<Version>,
    complete: bool,
}

fn select_versions(
    indexed: Vec<String>,
    current: &str,
    latest: &str,
) -> Result<VersionSelection, UpdateError> {
    let current = parse_stable_version(current, "current")?;
    let latest = parse_stable_version(latest, "latest")?;
    let mut versions = indexed
        .into_iter()
        .map(|value| parse_stable_version(&value, "indexed"))
        .collect::<Result<Vec<_>, _>>()?;
    versions.sort_unstable_by(|left, right| right.cmp(left));
    versions.dedup();
    versions.retain(|version| version > &current && version <= &latest);

    let mut complete = current >= latest || versions.first() == Some(&latest);
    if versions.len() > MAX_RELEASES {
        versions.truncate(MAX_RELEASES);
        complete = false;
    }
    Ok(VersionSelection { versions, complete })
}

fn parse_stable_version(value: &str, label: &str) -> Result<Version, UpdateError> {
    let version = Version::parse(value).map_err(|error| {
        UpdateError::ReleaseNotes(format!(
            "invalid {label} release version `{value}`: {error}"
        ))
    })?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(UpdateError::ReleaseNotes(format!(
            "{label} release version `{value}` is not stable"
        )));
    }
    Ok(version)
}

async fn fetch_one(ctx: &UpdateContext, version: &Version) -> Result<String, String> {
    let url = format!("{SITE_BASE_URL}/release-notes/v{version}.md");
    let bytes = http_get(ctx, &url, MAX_NOTES_BYTES).await?;
    String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("release notes for v{version} are not UTF-8: {error}"))
}

async fn http_get(ctx: &UpdateContext, url: &str, max_bytes: usize) -> Result<Bytes, String> {
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(url)
        .header(http::header::USER_AGENT, "gproxy-selfupdate")
        // A compressed Content-Length cannot bound the decoded body. Request
        // identity encoding and still enforce the limit on every stream chunk.
        .header(http::header::ACCEPT_ENCODING, "identity")
        .body(Bytes::new())
        .map_err(|error| format!("failed to build release notes request: {error}"))?;
    let (status, headers, stream) = ctx
        .client
        .send_streaming(request)
        .await
        .map_err(|error| format!("failed to fetch release notes: {error}"))?;
    if status != http::StatusCode::OK {
        return Err(format!("release notes request returned HTTP {status}"));
    }
    collect_bounded(&headers, stream, max_bytes).await
}

async fn collect_bounded(
    headers: &http::HeaderMap,
    mut stream: RespStream,
    max_bytes: usize,
) -> Result<Bytes, String> {
    let declared = headers
        .get(http::header::CONTENT_LENGTH)
        .map(|value| {
            value
                .to_str()
                .map_err(|error| format!("invalid release notes Content-Length: {error}"))?
                .parse::<u64>()
                .map_err(|error| format!("invalid release notes Content-Length: {error}"))
        })
        .transpose()?;
    if declared.is_some_and(|length| length > max_bytes as u64) {
        return Err(format!("release notes response exceeds {max_bytes} bytes"));
    }

    let capacity = declared.unwrap_or(0).min(max_bytes as u64) as usize;
    let mut body = BytesMut::with_capacity(capacity);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("failed to read release notes: {error}"))?;
        if chunk.len() > max_bytes.saturating_sub(body.len()) {
            return Err(format!("release notes response exceeds {max_bytes} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_interval_newest_first_and_marks_missing_latest() {
        let selected = select_versions(
            vec!["2.3.0".into(), "2.4.0".into(), "2.3.2".into()],
            "2.3.0",
            "2.4.0",
        )
        .unwrap();
        assert_eq!(
            selected
                .versions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["2.4.0", "2.3.2"]
        );
        assert!(selected.complete);

        let selected = select_versions(vec!["2.3.2".into()], "2.3.0", "2.4.0").unwrap();
        assert!(!selected.complete);
    }

    #[test]
    fn rejects_non_stable_index_entries() {
        let error = select_versions(vec!["2.4.0-rc.1".into()], "2.3.0", "2.4.0")
            .err()
            .unwrap();
        assert!(error.to_string().contains("not stable"));
    }

    #[tokio::test]
    async fn bounded_collection_checks_header_and_streamed_bytes() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_LENGTH,
            http::HeaderValue::from_static("6"),
        );
        let empty: RespStream = Box::pin(futures_util::stream::empty());
        assert!(collect_bounded(&headers, empty, 5).await.is_err());

        let headers = http::HeaderMap::new();
        let oversized: RespStream = Box::pin(futures_util::stream::iter([
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"def")),
        ]));
        assert!(collect_bounded(&headers, oversized, 5).await.is_err());

        let exact: RespStream = Box::pin(futures_util::stream::iter([
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"def")),
        ]));
        assert_eq!(
            collect_bounded(&headers, exact, 6).await.unwrap(),
            Bytes::from_static(b"abcdef")
        );
    }
}
