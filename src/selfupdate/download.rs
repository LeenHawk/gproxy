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

/// Fetch + parse the channel manifest.
pub async fn fetch_manifest(ctx: &UpdateContext) -> Result<Manifest, UpdateError> {
    let url = manifest_url(&ctx.repo, ctx.channel);
    let body = http_get(ctx, &url).await.map_err(UpdateError::Manifest)?;
    let text =
        String::from_utf8(body.to_vec()).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    Manifest::parse(&text).map_err(|e| UpdateError::Manifest(e.to_string()))
}

/// Download the artifact to `<data_dir>/.update/<sha-prefix>.tmp` and return the
/// staged path. Integrity/signature checks happen in [`super::verify`] before
/// this file is ever installed.
pub async fn download_artifact(
    ctx: &UpdateContext,
    artifact: &Artifact,
) -> Result<PathBuf, UpdateError> {
    let dir = ctx.data_dir.join(".update");
    std::fs::create_dir_all(&dir)?;
    restrict_dir(&dir)?;

    let prefix: String = artifact.sha256.chars().take(16).collect();
    let staged = dir.join(format!("{prefix}.tmp"));

    let body = http_get(ctx, &artifact.url)
        .await
        .map_err(UpdateError::Download)?;
    std::fs::write(&staged, &body)?;
    Ok(staged)
}

/// GET a URL via the upstream client, following one layer of redirect-style
/// errors is the caller's concern; here we just return the body or an error
/// string. Non-2xx is an error.
async fn http_get(ctx: &UpdateContext, url: &str) -> Result<Bytes, String> {
    let req = http::Request::builder()
        .method(http::Method::GET)
        .uri(url)
        .header(http::header::USER_AGENT, "gproxy-selfupdate")
        .body(Bytes::new())
        .map_err(|e| e.to_string())?;

    let resp = ctx
        .client
        .send(req)
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GET {url} returned HTTP {status}"));
    }
    Ok(resp.into_body())
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
}
