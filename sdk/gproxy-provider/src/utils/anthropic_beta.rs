use http::{HeaderMap, HeaderValue};

use crate::response::UpstreamError;

/// Ensure each of `tokens` is present in the `anthropic-beta` header.
///
/// `anthropic-beta` is a comma-separated list of capability flags
/// (`oauth-2025-04-20`, `files-api-2025-04-14`, `prompt-caching-2024-07-31`,
/// etc.). Anthropic accepts either a single comma-joined header value or
/// multiple separate header entries; we produce a single comma-joined
/// value because it's easier to inspect in logs and matches what the
/// claude-code CLI actually sends.
///
/// Any tokens already present in the existing header value are left
/// untouched (including unknown tokens a client supplied), so this
/// operation is idempotent and never clobbers caller intent. Missing
/// tokens are appended in the order given.
///
/// Used by the `anthropic` (direct Claude API) and `claudecode`
/// (Claude Code OAuth) channels to guarantee the right beta flags are
/// set on outbound requests without overwriting whatever the client or
/// a previous layer already requested.
pub fn ensure_anthropic_beta_tokens(
    headers: &mut HeaderMap,
    tokens: &[&str],
) -> Result<(), UpstreamError> {
    let existing = headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mut present: Vec<String> = existing
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let mut changed = false;
    for token in tokens {
        if !present.iter().any(|t| t == token) {
            present.push((*token).to_string());
            changed = true;
        }
    }
    if !changed && !existing.is_empty() {
        return Ok(());
    }
    let combined = present.join(",");
    let value = HeaderValue::from_str(&combined)
        .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
    headers.insert("anthropic-beta", value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_value(headers: &HeaderMap) -> String {
        headers
            .get("anthropic-beta")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn adds_token_when_header_absent() {
        let mut headers = HeaderMap::new();
        ensure_anthropic_beta_tokens(&mut headers, &["oauth-2025-04-20"]).unwrap();
        assert_eq!(header_value(&headers), "oauth-2025-04-20");
    }

    #[test]
    fn preserves_existing_tokens_and_appends_missing_ones() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("prompt-caching-2024-07-31,custom-flag"),
        );
        ensure_anthropic_beta_tokens(
            &mut headers,
            &["oauth-2025-04-20", "files-api-2025-04-14"],
        )
        .unwrap();
        assert_eq!(
            header_value(&headers),
            "prompt-caching-2024-07-31,custom-flag,oauth-2025-04-20,files-api-2025-04-14"
        );
    }

    #[test]
    fn idempotent_when_all_tokens_already_present() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("oauth-2025-04-20,files-api-2025-04-14"),
        );
        ensure_anthropic_beta_tokens(
            &mut headers,
            &["oauth-2025-04-20", "files-api-2025-04-14"],
        )
        .unwrap();
        assert_eq!(
            header_value(&headers),
            "oauth-2025-04-20,files-api-2025-04-14"
        );
    }

    #[test]
    fn trims_whitespace_in_existing_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("oauth-2025-04-20 , custom-flag"),
        );
        ensure_anthropic_beta_tokens(&mut headers, &["files-api-2025-04-14"]).unwrap();
        assert_eq!(
            header_value(&headers),
            "oauth-2025-04-20,custom-flag,files-api-2025-04-14"
        );
    }
}
