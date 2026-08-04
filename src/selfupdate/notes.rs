//! Best-effort release notes fetch from the project site.

use bytes::Bytes;

use super::{Channel, UpdateContext};
use crate::site::SITE_BASE_URL;

const MAX_NOTES_BYTES: usize = 64 * 1024;

pub async fn fetch(ctx: &UpdateContext, manifest_version: &str) -> Option<String> {
    if ctx.channel == Channel::Staging {
        return None;
    }
    let url = format!("{SITE_BASE_URL}/release-notes/v{manifest_version}.md");
    let request = match http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
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

fn extract_body(bytes: &[u8]) -> Result<String, std::string::FromUtf8Error> {
    let mut body = String::from_utf8(bytes.to_vec())?;
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
    fn extracts_markdown_body() {
        assert_eq!(extract_body(b"# Release notes").unwrap(), "# Release notes");
    }

    #[test]
    fn truncates_on_a_utf8_boundary() {
        let body = format!("{}界", "a".repeat(MAX_NOTES_BYTES - 1));
        let notes = extract_body(body.as_bytes()).unwrap();

        assert_eq!(notes.len(), MAX_NOTES_BYTES - 1);
        assert!(notes.is_char_boundary(notes.len()));
    }
}
