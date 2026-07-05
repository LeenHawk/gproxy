//! Manifest + artifact download (§19.5) — NATIVE only.
//!
//! Uses the proxy-aware [`UpstreamClient`] (the same transport the proxy data
//! plane uses) to fetch the channel manifest and the platform artifact. Staged
//! files land under `<data_dir>/.update` — the same filesystem as the binary so
//! the later `rename` is atomic.

use std::path::PathBuf;

use bytes::Bytes;

use super::manifest::{Artifact, Manifest};
use super::{Channel, UpdateContext, UpdateError};

/// Resolve the manifest URL for the channel (§19.3). The manifest is published
/// as a release asset named `manifest.json`.
///
/// - `releases`: `.../releases/latest/download/manifest.json`
/// - `staging`:  `.../releases/download/staging/manifest.json`
fn manifest_url(repo: &str, channel: Channel) -> String {
    match channel {
        Channel::Releases => {
            format!("https://github.com/{repo}/releases/latest/download/manifest.json")
        }
        Channel::Staging => {
            format!("https://github.com/{repo}/releases/download/staging/manifest.json")
        }
    }
}

/// Fetch + parse the channel manifest. A 404 (no manifest published for the
/// channel) is surfaced as the dedicated [`UpdateError::ManifestNotFound`] so
/// callers can treat "no update info" as a benign state rather than a 500.
pub async fn fetch_manifest(ctx: &UpdateContext) -> Result<Manifest, UpdateError> {
    let url = manifest_url(&ctx.repo, ctx.channel);
    let body = http_get(ctx, &url).await.map_err(|e| match e {
        HttpGetError::NotFound => UpdateError::ManifestNotFound,
        HttpGetError::Other(m) => UpdateError::Manifest(m),
    })?;
    let text =
        String::from_utf8(body.to_vec()).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    Manifest::parse(&text).map_err(|e| UpdateError::Manifest(e.to_string()))
}

/// Download the artifact to `<data_dir>/.update/<sha-prefix>.tmp` and return the
/// staged path. If the manifest has no sha256, the caller must already have
/// required explicit operator confirmation and the temp file uses an
/// `unverified` prefix.
pub async fn download_artifact(
    ctx: &UpdateContext,
    artifact: &Artifact,
) -> Result<PathBuf, UpdateError> {
    let dir = ctx.data_dir.join(".update");
    std::fs::create_dir_all(&dir)?;
    restrict_dir(&dir)?;

    let prefix: String = artifact
        .sha256_value()
        .map(|sha| sha.chars().take(16).collect())
        .unwrap_or_else(|| "unverified".to_string());
    let staged = dir.join(format!("{prefix}.tmp"));

    let body = http_get(ctx, &artifact.url)
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?;
    std::fs::write(&staged, &body)?;
    Ok(staged)
}

/// Error from [`http_get`]: a 404 is distinguished so the manifest fetch can map
/// it to [`UpdateError::ManifestNotFound`]; everything else is opaque.
#[derive(Debug)]
enum HttpGetError {
    NotFound,
    Other(String),
}

impl std::fmt::Display for HttpGetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpGetError::NotFound => write!(f, "not found (HTTP 404)"),
            HttpGetError::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Max redirect hops to follow (GitHub release downloads bounce
/// `releases/latest/download/...` → `releases/download/<tag>/...` →
/// `objects.githubusercontent.com/...`, ~2-3 hops).
const MAX_REDIRECTS: usize = 6;

/// GET a URL via the upstream client, **following redirects** (the shared proxy
/// client does not follow them — and must not — so self-update follows them
/// here). A 404 at the final hop maps to [`HttpGetError::NotFound`]; any other
/// non-2xx (or transport error) maps to [`HttpGetError::Other`].
async fn http_get(ctx: &UpdateContext, url: &str) -> Result<Bytes, HttpGetError> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(&current)
            .header(http::header::USER_AGENT, "gproxy-selfupdate")
            .body(Bytes::new())
            .map_err(|e| HttpGetError::Other(e.to_string()))?;

        let resp = ctx
            .client
            .send(req)
            .await
            .map_err(|e| HttpGetError::Other(format!("GET {current}: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(http::header::LOCATION)
                .and_then(|v| v.to_str().ok());
            match location {
                Some(loc) => {
                    current = resolve_redirect(&current, loc)?;
                    continue;
                }
                None => {
                    return Err(HttpGetError::Other(format!(
                        "redirect {status} without Location header"
                    )));
                }
            }
        }
        if status == http::StatusCode::NOT_FOUND {
            return Err(HttpGetError::NotFound);
        }
        if !status.is_success() {
            return Err(HttpGetError::Other(format!(
                "GET {current} returned HTTP {status}"
            )));
        }
        return Ok(resp.into_body());
    }
    Err(HttpGetError::Other(format!(
        "too many redirects (>{MAX_REDIRECTS}) fetching {url}"
    )))
}

/// Resolve a `Location` value against the URL it came from. Absolute URLs are
/// used as-is; absolute-path (`/...`) references are rebased onto the current
/// scheme+authority (enough for GitHub's redirect chain).
fn resolve_redirect(base: &str, location: &str) -> Result<String, HttpGetError> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    let base_uri: http::Uri = base
        .parse()
        .map_err(|e| HttpGetError::Other(format!("bad base URL {base}: {e}")))?;
    let scheme = base_uri.scheme_str().unwrap_or("https");
    let authority = base_uri
        .authority()
        .map(|a| a.as_str())
        .ok_or_else(|| HttpGetError::Other(format!("base URL has no host: {base}")))?;
    if let Some(abs_path) = location.strip_prefix('/') {
        Ok(format!("{scheme}://{authority}/{abs_path}"))
    } else {
        Err(HttpGetError::Other(format!(
            "unsupported relative redirect `{location}` from {base}"
        )))
    }
}

/// Restrict the staging dir to the owner (chmod 700, §19.10). Unix-only; a
/// no-op elsewhere.
#[cfg(unix)]
fn restrict_dir(dir: &std::path::Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(dir, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &std::path::Path) -> Result<(), UpdateError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_urls_match_channel() {
        assert_eq!(
            manifest_url("acme/gproxy", Channel::Releases),
            "https://github.com/acme/gproxy/releases/latest/download/manifest.json"
        );
        assert_eq!(
            manifest_url("acme/gproxy", Channel::Staging),
            "https://github.com/acme/gproxy/releases/download/staging/manifest.json"
        );
    }

    #[test]
    fn resolve_redirect_handles_absolute_and_rooted() {
        // Absolute Location (GitHub → objects.githubusercontent.com) is used as-is.
        assert_eq!(
            resolve_redirect(
                "https://github.com/o/r/releases/latest/download/manifest.json",
                "https://objects.githubusercontent.com/x/y?token=abc"
            )
            .unwrap(),
            "https://objects.githubusercontent.com/x/y?token=abc"
        );
        // Absolute-path Location is rebased onto the current scheme+host.
        assert_eq!(
            resolve_redirect(
                "https://github.com/o/r/releases/latest/download/manifest.json",
                "/o/r/releases/download/v2.0.6/manifest.json"
            )
            .unwrap(),
            "https://github.com/o/r/releases/download/v2.0.6/manifest.json"
        );
        // A non-rooted relative ref is rejected (never emitted by GitHub).
        assert!(resolve_redirect("https://github.com/a/b", "somewhere").is_err());
    }
}
