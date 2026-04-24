//! Self-update: check for new version, download, verify signature, replace binary, restart.
//!
//! Update assets follow a uniform URL pattern on GitHub releases:
//!   `{GITHUB_DOWNLOAD_BASE}/{tag}/gproxy-{platform}-{arch}.zip`
//!   `{GITHUB_DOWNLOAD_BASE}/{tag}/gproxy-{platform}-{arch}.zip.sha256`
//!   `{GITHUB_DOWNLOAD_BASE}/{tag}/gproxy-{platform}-{arch}.zip.sha256.sig`
//!
//! GitHub is the only supported source; the latest release is discovered via
//! the GitHub Releases API.

use std::io::Read as _;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use gproxy_server::AppState;

use crate::auth::authorize_admin;
use crate::error::HttpError;

// ---------------------------------------------------------------------------
// Compile-time constants
// ---------------------------------------------------------------------------

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub release download base.
const GITHUB_DOWNLOAD_BASE: &str = "https://github.com/LeenHawk/gproxy/releases/download";

/// Ed25519 public key for verifying update signatures (base64-encoded).
///
/// Design note:
/// Self-compiled builds may intentionally omit this key so operators can point a
/// custom build at the official release stream without rebuilding again just to
/// embed the official signing key. In that mode, self-update falls back to
/// checksum-only verification against GitHub.
///
/// Deployments that require signed updates must compile with
/// `GPROXY_UPDATE_SIGN_PUBLIC_KEY_B64` set.
const UPDATE_SIGNING_PUBLIC_KEY_B64: Option<&str> =
    option_env!("GPROXY_UPDATE_SIGN_PUBLIC_KEY_B64");

/// Platform asset name component, e.g. "linux-x86_64", "macos-aarch64".
fn platform_asset_name() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "unknown"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    format!("{os}-{arch}")
}

fn asset_filename() -> String {
    format!("gproxy-{}.zip", platform_asset_name())
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub download_url: Option<String>,
}

#[derive(Serialize)]
pub struct UpdatePerformResponse {
    pub ok: bool,
    pub old_version: String,
    pub new_version: String,
    pub message: String,
}

#[derive(Deserialize, Default)]
pub struct UpdateParams {
    #[serde(default)]
    pub tag: Option<String>,
}

// ---------------------------------------------------------------------------
// Version manifest — discovered via GitHub Releases API
// ---------------------------------------------------------------------------

struct VersionManifest {
    version: String,
    tag: String,
}

// ---------------------------------------------------------------------------
// Check for update
// ---------------------------------------------------------------------------

pub async fn check_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<UpdateCheckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    let client = state.engine().client().clone();
    let channel = state.config().update_channel;
    let (latest_version, tag) = match fetch_github_manifest(&client, channel).await {
        Ok(m) => (Some(m.version), Some(m.tag)),
        Err(e) => {
            tracing::warn!(error = %e, channel = ?channel, "failed to fetch update manifest");
            (None, None)
        }
    };

    let update_available = if matches!(channel, gproxy_core::UpdateChannel::Staging) {
        // Staging is a moving tag — always treat as updatable so the UI
        // surfaces a download button for fresh staging builds.
        latest_version.is_some()
    } else {
        latest_version
            .as_ref()
            .is_some_and(|v| is_newer_version(CURRENT_VERSION, v))
    };

    let download_url = tag.map(|t| {
        format!(
            "{}/{}/{}",
            GITHUB_DOWNLOAD_BASE.trim_end_matches('/'),
            t,
            asset_filename()
        )
    });

    Ok(Json(UpdateCheckResponse {
        current_version: CURRENT_VERSION.to_string(),
        latest_version,
        update_available,
        download_url,
    }))
}

// ---------------------------------------------------------------------------
// Perform update
// ---------------------------------------------------------------------------

pub async fn perform_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(params): Json<UpdateParams>,
) -> Result<Json<UpdatePerformResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    let client = state.engine().client().clone();

    // Determine tag: explicit param, or fetch latest manifest
    let tag = if let Some(t) = params.tag {
        t
    } else {
        let channel = state.config().update_channel;
        let manifest = fetch_github_manifest(&client, channel)
            .await
            .map_err(|e| HttpError::internal(format!("failed to check for updates: {e}")))?;
        if !matches!(channel, gproxy_core::UpdateChannel::Staging)
            && !is_newer_version(CURRENT_VERSION, &manifest.version)
        {
            return Ok(Json(UpdatePerformResponse {
                ok: true,
                old_version: CURRENT_VERSION.to_string(),
                new_version: manifest.version,
                message: "already up to date".to_string(),
            }));
        }
        manifest.tag
    };

    let base = GITHUB_DOWNLOAD_BASE.trim_end_matches('/');
    let asset = asset_filename();
    let zip_url = format!("{}/{}/{}", base, tag, asset);
    let sha_url = format!("{}.sha256", zip_url);
    let sig_url = format!("{}.sha256.sig", zip_url);

    tracing::info!(version = %tag, url = %zip_url, "downloading update");

    // Download zip
    let zip_bytes = download_bytes(&client, &zip_url)
        .await
        .map_err(|e| HttpError::internal(format!("download failed: {e}")))?;

    // Verify SHA256
    let sha_content = download_text(&client, &sha_url)
        .await
        .map_err(|e| HttpError::internal(format!("SHA256 checksum download failed: {e}")))?;
    let actual_sha = hex_sha256(&zip_bytes);
    let expected = sha_content.split_whitespace().next().unwrap_or("").trim();
    if actual_sha != expected {
        return Err(HttpError::internal(format!(
            "SHA256 mismatch: expected {expected}, got {actual_sha}"
        )));
    }
    tracing::info!("SHA256 verified");

    // Verify Ed25519 signature
    if let Some(pub_key_b64) = UPDATE_SIGNING_PUBLIC_KEY_B64 {
        let sig_bytes = download_bytes(&client, &sig_url)
            .await
            .map_err(|e| HttpError::internal(format!("signature download failed: {e}")))?;
        verify_ed25519(pub_key_b64, sha_content.as_bytes(), &sig_bytes)
            .map_err(|e| HttpError::internal(format!("signature verification failed: {e}")))?;
        tracing::info!("Ed25519 signature verified");
    } else {
        // Intentional fallback for self-compiled builds without an embedded
        // signing key. This keeps the update path usable against the official
        // release stream, but integrity then depends on the checksum file and
        // trust in GitHub.
        tracing::warn!("no signing public key compiled in, skipping signature verification");
    }

    // Extract binary from zip
    let binary = extract_binary_from_zip(&zip_bytes)
        .map_err(|e| HttpError::internal(format!("zip extraction failed: {e}")))?;

    // Replace current executable
    let exe_path = std::env::current_exe()
        .map_err(|e| HttpError::internal(format!("cannot determine executable path: {e}")))?;
    replace_executable(&exe_path, &binary)
        .map_err(|e| HttpError::internal(format!("binary replacement failed: {e}")))?;

    tracing::info!(path = %exe_path.display(), "binary replaced, scheduling restart");

    let new_version = tag.strip_prefix('v').unwrap_or(&tag).to_string();
    let response = UpdatePerformResponse {
        ok: true,
        old_version: CURRENT_VERSION.to_string(),
        new_version,
        message: "updated, restarting...".to_string(),
    };

    // Schedule restart after response is sent
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        restart_process();
    });

    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn fetch_github_manifest(
    client: &wreq::Client,
    channel: gproxy_core::UpdateChannel,
) -> Result<VersionManifest, String> {
    match channel {
        gproxy_core::UpdateChannel::Staging => {
            // Long-lived GitHub release tagged literally `staging`,
            // refreshed continuously. Skip the manifest fetch and return
            // the tag directly.
            Ok(VersionManifest {
                version: "staging".to_string(),
                tag: "staging".to_string(),
            })
        }
        gproxy_core::UpdateChannel::Release => fetch_latest_release(client).await,
    }
}

async fn fetch_latest_release(client: &wreq::Client) -> Result<VersionManifest, String> {
    let api_url = "https://api.github.com/repos/LeenHawk/gproxy/releases/latest";
    let resp = client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "gproxy-updater")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct GithubRelease {
        tag_name: String,
    }

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {e}"))?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name)
        .to_string();
    Ok(VersionManifest {
        version,
        tag: release.tag_name,
    })
}

async fn download_bytes(client: &wreq::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "gproxy-updater")
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("read body failed: {e}"))
}

async fn download_text(client: &wreq::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "gproxy-updater")
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("read body failed: {e}"))
}

fn hex_sha256(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

fn verify_ed25519(pub_key_b64: &str, message: &[u8], signature: &[u8]) -> Result<(), String> {
    use base64::Engine;
    use ed25519_dalek::{Signature, VerifyingKey};

    let pub_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(pub_key_b64)
        .map_err(|e| format!("invalid public key base64: {e}"))?;
    let pub_key_array: [u8; 32] = pub_key_bytes
        .try_into()
        .map_err(|_| "public key must be 32 bytes")?;
    let verifying_key =
        VerifyingKey::from_bytes(&pub_key_array).map_err(|e| format!("invalid public key: {e}"))?;

    // Try parsing signature as raw bytes (64), hex (128 chars), or base64
    let sig = if signature.len() == 64 {
        Signature::from_bytes(
            signature
                .try_into()
                .map_err(|_| "signature must be 64 bytes")?,
        )
    } else if let Ok(s) = std::str::from_utf8(signature) {
        let trimmed = s.trim();
        if trimmed.len() == 128 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            // Hex
            let mut bytes = [0u8; 64];
            for i in 0..64 {
                bytes[i] = u8::from_str_radix(&trimmed[i * 2..i * 2 + 2], 16)
                    .map_err(|e| format!("invalid hex in signature: {e}"))?;
            }
            Signature::from_bytes(&bytes)
        } else {
            // Base64
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(trimmed)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
                .map_err(|e| format!("invalid signature encoding: {e}"))?;
            Signature::from_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| "signature must be 64 bytes")?,
            )
        }
    } else {
        return Err("unrecognized signature format".to_string());
    };

    use ed25519_dalek::Verifier;
    verifying_key
        .verify(message, &sig)
        .map_err(|e| format!("signature invalid: {e}"))
}

fn extract_binary_from_zip(zip_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("invalid zip: {e}"))?;

    let binary_name = if cfg!(windows) {
        "gproxy.exe"
    } else {
        "gproxy"
    };

    // Try exact name first, then any file containing the name
    let idx = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .ok()
                .is_some_and(|f| f.name().ends_with(binary_name))
        })
        .ok_or_else(|| format!("'{binary_name}' not found in zip"))?;

    let mut file = archive
        .by_index(idx)
        .map_err(|e| format!("zip read: {e}"))?;
    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| format!("extract: {e}"))?;
    Ok(buf)
}

fn replace_executable(exe_path: &std::path::Path, new_binary: &[u8]) -> Result<(), String> {
    let tmp_path = exe_path.with_extension("new");

    // Write new binary to temp file
    std::fs::write(&tmp_path, new_binary).map_err(|e| format!("write temp: {e}"))?;

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod: {e}"))?;
    }

    // Atomic rename (same filesystem)
    #[cfg(unix)]
    {
        std::fs::rename(&tmp_path, exe_path).map_err(|e| format!("rename: {e}"))?;
    }

    // Windows: rename old → .bak, new → old
    #[cfg(windows)]
    {
        let bak_path = exe_path.with_extension("bak");
        let _ = std::fs::remove_file(&bak_path);
        std::fs::rename(exe_path, &bak_path).map_err(|e| format!("backup: {e}"))?;
        std::fs::rename(&tmp_path, exe_path).map_err(|e| format!("replace: {e}"))?;
    }

    Ok(())
}

fn restart_process() -> ! {
    let exe = std::env::current_exe().expect("current_exe");
    let args: Vec<String> = std::env::args().collect();

    tracing::info!("restarting process");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&exe).args(&args[1..]).exec();
        tracing::error!(error = %err, "exec failed");
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new(&exe).args(&args[1..]).spawn();
        std::process::exit(0);
    }
}

/// Simple semver comparison: "1.2.3" > "1.2.2"
fn is_newer_version(current: &str, latest: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let s = s.strip_prefix('v').unwrap_or(s);
        let mut parts = s.split('.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts
            .next()
            .and_then(|p| p.split('-').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        (major, minor, patch)
    };
    parse(latest) > parse(current)
}
