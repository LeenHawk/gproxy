//! The release notes, when the manifest names somewhere to read them.
//!
//! # A nicety, and treated as one
//!
//! Every function here returns `Option` and none returns `Result`. Notes are
//! what a human reads before pressing apply; a release whose notes cannot be
//! fetched is still a release, and an updater that refused to report a new
//! version because `api.github.com` was slow would be failing at its one job.
//! So a failure here is silence, not an error.
//!
//! The URL comes from the manifest and is therefore **signed** — see
//! [`super::manifest`] — so the host being asked is the one the release
//! publisher named, not one a manifest mirror inserted.
//!
//! # The shape it expects, and what the pipeline currently sends
//!
//! GitHub's releases *API* answers `{"body": "…"}`, and that is the one shape
//! this parses. What `.github/workflows/release.yml` currently puts in
//! `NOTES_URL` is the release's **HTML page**
//! (`/releases/tag/{tag}`), which is not that shape — so against today's
//! pipeline this returns `None` and the URL itself is what a console links to
//! and what `gproxy update --check` prints.
//!
//! That is a deliberate order of preference rather than a gap. The link is the
//! thing an operator must have before installing, it is covered by the
//! manifest's signature, and it costs nothing; the text is a convenience that
//! appears the day the pipeline points `NOTES_URL` at
//! `api.github.com/repos/{repo}/releases/tags/{tag}` instead. Nothing here
//! needs to change for that to start working.

use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    body: Option<String>,
}

/// The longest notes worth holding in a status a console polls. A release
/// description longer than this is a document, not a summary, and the URL is
/// the right answer for it.
const MAX_BYTES: usize = 64 * 1024;

pub(super) async fn fetch(client: &reqwest::Client, url: &str) -> Option<String> {
    // Only over HTTP(S): a signed `file:///` notes URL would still be the
    // release publisher's, but reading a local path because a remote document
    // said to is not a thing this process should learn how to do.
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let body = serde_json::from_slice::<Release>(&bytes).ok()?.body?;
    let trimmed = body.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_url_that_is_not_http_is_never_fetched() {
        let client = super::super::download::client().unwrap();
        // No server is running, and none is needed: the scheme check happens
        // before any request. A `file://` URL that did reach the client would
        // be an error rather than `None`, so this also proves the order.
        assert!(super::fetch(&client, "file:///etc/passwd").await.is_none());
        assert!(super::fetch(&client, "").await.is_none());
    }
}
