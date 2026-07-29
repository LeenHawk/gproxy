//! Best-effort auto display-name for credentials, derived from the PLAINTEXT
//! secret at creation time. Shared by the admin upsert path, OAuth login flows
//! and the CLI bundle import so all entrances name credentials identically.

use serde_json::Value;

/// Derive a display name when the caller supplied none. Returns `None` when
/// nothing identifying can be extracted — the stored name stays NULL and the
/// UI falls back to "Unnamed #id".
pub fn auto_label(kind: &str, secret: &Value) -> Option<String> {
    match kind {
        "api_key" => token_field(secret, "api_key").and_then(mask),
        "github_token" => token_field(secret, "github_token").and_then(mask),
        "service_account" => str_field(secret, "client_email").map(str::to_string),
        // OAuth-style secrets ("oauth", "oauth_tokens", cookies, …): prefer an
        // account identity over token material.
        _ => identity_label(secret),
    }
}

/// The bare token for single-token kinds: a bare string secret or the named
/// object field.
fn token_field<'a>(secret: &'a Value, field: &str) -> Option<&'a str> {
    match secret {
        Value::String(s) => non_empty(s),
        Value::Object(obj) => obj.get(field).and_then(Value::as_str).and_then(non_empty),
        _ => None,
    }
}

/// Mask a token to `head…tail` (5+4 chars); short tokens keep only the tail,
/// and anything under 8 chars is too short to name meaningfully.
fn mask(token: &str) -> Option<String> {
    let chars: Vec<char> = token.chars().collect();
    let tail: String = chars.iter().rev().take(4).rev().collect();
    match chars.len() {
        0..=7 => None,
        8..=11 => Some(format!("…{tail}")),
        _ => {
            let head: String = chars.iter().take(5).collect();
            Some(format!("{head}…{tail}"))
        }
    }
}

/// Account identity from an OAuth-style secret: `user_email` (optionally with
/// `rate_limit_tier`, matching the login-flow convention), then `email`, then
/// `account_id`.
fn identity_label(secret: &Value) -> Option<String> {
    if let Some(email) = str_field(secret, "user_email") {
        return Some(match str_field(secret, "rate_limit_tier") {
            Some(tier) => format!("{email} {tier}"),
            None => email.to_string(),
        });
    }
    str_field(secret, "email")
        .or_else(|| str_field(secret, "account_id"))
        .map(str::to_string)
}

fn str_field<'a>(secret: &'a Value, field: &str) -> Option<&'a str> {
    secret
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(non_empty)
}

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::auto_label;
    use serde_json::json;

    #[test]
    fn labels_per_kind() {
        assert_eq!(
            auto_label("api_key", &json!({ "api_key": "sk-proj-abcdef7890" })),
            Some("sk-pr…7890".into())
        );
        assert_eq!(auto_label("api_key", &json!({ "api_key": "short" })), None);
        assert_eq!(
            auto_label("github_token", &json!({ "github_token": "ghu_toklm" })),
            Some("…oklm".into())
        );
        assert_eq!(
            auto_label(
                "service_account",
                &json!({ "client_email": "svc@proj.iam" })
            ),
            Some("svc@proj.iam".into())
        );
        assert_eq!(
            auto_label(
                "oauth",
                &json!({ "user_email": "a@b.c", "rate_limit_tier": "pro" })
            ),
            Some("a@b.c pro".into())
        );
        assert_eq!(
            auto_label("oauth_tokens", &json!({ "account_id": "acct_1" })),
            Some("acct_1".into())
        );
        assert_eq!(auto_label("oauth", &json!({ "access_token": "x" })), None);
    }
}
