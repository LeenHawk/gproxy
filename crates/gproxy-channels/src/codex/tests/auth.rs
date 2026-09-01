use base64::Engine as _;
use serde_json::json;

#[test]
fn login_secret_retains_jwt_account_identity() {
    let claims = json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acct-1",
            "chatgpt_account_is_fedramp": true
        }
    });
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    let secret = super::super::auth::login_secret(&json!({
        "access_token": "access",
        "refresh_token": "refresh",
        "id_token": format!("header.{payload}.signature")
    }))
    .unwrap();

    assert_eq!(secret["user_email"], "user@example.com");
    assert_eq!(secret["account_id"], "acct-1");
    assert_eq!(secret["chatgpt_account_is_fedramp"], true);
}
