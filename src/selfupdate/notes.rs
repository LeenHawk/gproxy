//! Best-effort GitHub Release notes fetch.

use bytes::Bytes;
use serde::Deserialize;

use super::{Channel, UpdateContext};

const MAX_NOTES_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
struct GitHubRelease {
    body: String,
}

pub async fn fetch(ctx: &UpdateContext, manifest_version: &str) -> Option<String> {
    let tag = match ctx.channel {
        Channel::Releases => format!("v{manifest_version}"),
        Channel::Staging => "staging".to_string(),
    };
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{tag}",
        ctx.repo
    );
    let request = match http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header(http::header::ACCEPT, "application/vnd.github+json")
        .header(http::header::USER_AGENT, "gproxy-selfupdate")
        .body(Bytes::new())
    {
        Ok(request) => request,
        Err(error) => {
            tracing::debug!(%error, "failed to build release notes request");
            return None;
        }
    };

    let response = match ctx.client.send(request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(%error, "failed to fetch release notes");
            return None;
        }
    };
    if response.status() != http::StatusCode::OK {
        tracing::debug!(status = %response.status(), "release notes request was unsuccessful");
        return None;
    }

    match extract_body(response.body()) {
        Ok(body) => Some(body),
        Err(error) => {
            tracing::debug!(%error, "failed to parse release notes response");
            None
        }
    }
}

fn extract_body(json: &[u8]) -> Result<String, serde_json::Error> {
    let mut body = serde_json::from_slice::<GitHubRelease>(json)?.body;
    if body.len() > MAX_NOTES_BYTES {
        let mut end = MAX_NOTES_BYTES;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_release_body() {
        assert_eq!(
            extract_body(br#"{"body":"release notes"}"#).unwrap(),
            "release notes"
        );
    }

    #[test]
    fn truncates_on_a_utf8_boundary() {
        let body = format!("{}界", "a".repeat(MAX_NOTES_BYTES - 1));
        let json = serde_json::to_vec(&serde_json::json!({ "body": body })).unwrap();
        let notes = extract_body(&json).unwrap();

        assert_eq!(notes.len(), MAX_NOTES_BYTES - 1);
        assert!(notes.is_char_boundary(notes.len()));
    }
}
