//! Fetching the manifest and the artifact, and refusing anything that does not
//! match what the signature covered.
//!
//! # Order of checks, and why it is that order
//!
//! 1. the HTTP status, because a 404 body is not a manifest;
//! 2. for the manifest: the **signature**, before any field of it is read as
//!    an instruction — [`Manifest::parse_verified`] is the only constructor, so
//!    this cannot be skipped;
//! 3. for the artifact: the **size**, then the **hash**. Size first because a
//!    truncated or padded download is the common failure and comparing one
//!    integer is cheaper than hashing eighteen megabytes to learn the same
//!    thing.
//!
//! Nothing is written to disk in this module. The bytes are held in memory and
//! handed to [`super::extract`] only after both checks passed, so a failed
//! download leaves no partial file for anything to pick up later.

use sha2::{Digest as _, Sha256};

use super::config::UpdateError;
use super::manifest::{Artifact, Manifest};

/// The `User-Agent` a manifest host sees. Names the component rather than the
/// product, so a release host can tell an updater's poll from a person's
/// browser in its logs.
pub(super) const USER_AGENT: &str = concat!("gproxy-update/", env!("CARGO_PKG_VERSION"));

/// A client for the two requests this module makes.
///
/// Redirects are followed, which is not incidental: a GitHub release asset URL
/// is a 302 to object storage, so an updater that did not follow one could
/// never download anything. It is safe here for the same reason it is safe
/// everywhere in this module — the signature and the hash are checked after
/// the bytes arrive, so where they arrived from does not have to be trusted.
pub(super) fn client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // A manifest is a few kilobytes and an artifact is tens of megabytes;
        // the timeout is generous enough for the second on a slow link and
        // still bounded, so a scheduled check cannot wedge on a hung socket
        // until the process restarts.
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|_| UpdateError::Download)
}

pub(super) async fn manifest(
    client: &reqwest::Client,
    url: &str,
    key: Option<&str>,
) -> Result<Manifest, UpdateError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| UpdateError::Download)?;
    if !response.status().is_success() {
        return Err(UpdateError::Download);
    }
    let bytes = response.bytes().await.map_err(|_| UpdateError::Download)?;
    Manifest::parse_verified(&bytes, key)
}

pub(super) async fn artifact(
    client: &reqwest::Client,
    artifact: &Artifact,
) -> Result<Vec<u8>, UpdateError> {
    let response = client
        .get(&artifact.url)
        .send()
        .await
        .map_err(|_| UpdateError::Download)?;
    if !response.status().is_success() {
        return Err(UpdateError::Download);
    }
    let bytes = response.bytes().await.map_err(|_| UpdateError::Download)?;
    verify(&bytes, artifact)?;
    Ok(bytes.to_vec())
}

/// The size and the hash the manifest's signature covered.
pub(super) fn verify(bytes: &[u8], artifact: &Artifact) -> Result<(), UpdateError> {
    if bytes.len() as u64 != artifact.size {
        return Err(UpdateError::Integrity);
    }
    let actual = hex(&Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(artifact.sha256.trim()) {
        return Err(UpdateError::Integrity);
    }
    Ok(())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact_for(bytes: &[u8]) -> Artifact {
        // Deserialized rather than constructed, because the fields are
        // `pub(super)` to this module's parent and a literal here would be a
        // second definition of the shape to keep in step.
        serde_json::from_value(serde_json::json!({
            "target_triple": "test",
            "url": "https://example.test/gproxy.zip",
            "sha256": hex(&Sha256::digest(bytes)),
            "size": bytes.len(),
        }))
        .unwrap()
    }

    #[test]
    fn the_hash_and_the_size_are_both_checked() {
        let bytes = b"verified update bytes";
        let good = artifact_for(bytes);
        verify(bytes, &good).unwrap();

        // A byte changed, keeping the length: only the hash catches it.
        let mut flipped = bytes.to_vec();
        flipped[0] ^= 0xff;
        assert!(matches!(
            verify(&flipped, &good),
            Err(UpdateError::Integrity)
        ));

        // Truncated: the size catches it before the hash is computed.
        assert!(matches!(
            verify(&bytes[..bytes.len() - 1], &good),
            Err(UpdateError::Integrity)
        ));

        // And an empty download, which is what a proxy serving an error page
        // with a 200 often amounts to.
        assert!(matches!(verify(b"", &good), Err(UpdateError::Integrity)));
    }

    #[test]
    fn the_hash_comparison_is_case_insensitive() {
        let bytes = b"verified update bytes";
        let mut artifact = artifact_for(bytes);
        artifact.sha256 = artifact.sha256.to_ascii_uppercase();
        verify(bytes, &artifact).unwrap();
        // And surrounding whitespace, which is what a `.sha256` sidecar read
        // with `awk` can leave behind.
        artifact.sha256 = format!("  {}\n", artifact.sha256);
        verify(bytes, &artifact).unwrap();
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
