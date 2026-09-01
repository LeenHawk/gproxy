//! Best-effort auto display-name for credentials, derived from the plaintext
//! secret at creation time. Shared by every control-plane creation path so
//! credentials are named identically before their secret is sealed.

use serde_json::Value;

/// Derive a display name when the caller supplied none. `None` means nothing
/// identifying could be extracted — the label stays NULL and the console
/// falls back to "Credential #id".
pub fn auto_label(kind: &str, secret: &Value) -> Option<String> {
    // An account identity beats token material for every kind: OAuth and
    // cookie logins carry user_email/account_id, service-account JSON carries
    // client_email — even when stored under the api_key kind.
    identity(secret).or_else(|| match kind {
        "api_key" => token(secret).and_then(mask),
        _ => None,
    })
}

/// `user_email` keeps its `rate_limit_tier` suffix when present, matching the
/// login-flow convention; then `email`, `client_email`, `account_id`.
fn identity(secret: &Value) -> Option<String> {
    if let Some(email) = field(secret, "user_email") {
        return Some(match field(secret, "rate_limit_tier") {
            Some(tier) => format!("{email} {tier}"),
            None => email.to_owned(),
        });
    }
    field(secret, "email")
        .or_else(|| field(secret, "client_email"))
        .or_else(|| field(secret, "account_id"))
        .map(str::to_owned)
}

/// The bare token: a bare string secret, or the token-bearing object field.
fn token(secret: &Value) -> Option<&str> {
    match secret {
        Value::String(value) => non_empty(value),
        Value::Object(_) => field(secret, "api_key").or_else(|| field(secret, "github_token")),
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

fn field<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(non_empty)
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::auto_label;

    #[test]
    fn labels_per_kind() {
        assert_eq!(
            auto_label("api_key", &json!({ "api_key": "sk-proj-abcdef7890" })),
            Some("sk-pr…7890".into())
        );
        assert_eq!(auto_label("api_key", &json!({ "api_key": "short" })), None);
        assert_eq!(
            auto_label("api_key", &json!({ "github_token": "ghu_toklm" })),
            Some("…oklm".into())
        );
        assert_eq!(
            auto_label("api_key", &json!({ "client_email": "svc@proj.iam" })),
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
            auto_label("cookie", &json!({ "user_email": "a@b.c" })),
            Some("a@b.c".into())
        );
        assert_eq!(
            auto_label("oauth", &json!({ "account_id": "acct_1" })),
            Some("acct_1".into())
        );
        assert_eq!(auto_label("oauth", &json!({ "access_token": "x" })), None);
    }
}
