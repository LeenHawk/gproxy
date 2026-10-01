#![cfg(any(feature = "cline", feature = "kimi", feature = "opencode"))]

use gproxy_channel::BaseChannel;
use serde_json::json;

mod support;

fn check_mixed_credentials(channel: &dyn BaseChannel) {
    let refresh = channel.credential_refresh().unwrap();
    // Imported credentials can retain a legacy auth_kind; inspect the material.
    let key = json!({"api_key": "key"});
    let login = json!({"access_token": "access", "refresh_token": "refresh"});
    assert!(!refresh.supports(&support::credential("oauth", &key, &json!({}))));
    assert!(refresh.supports(&support::credential("api_key", &login, &json!({}))));
}

#[test]
#[cfg(feature = "cline")]
fn cline_refresh_requires_renewable_credentials() {
    check_mixed_credentials(&gproxy_channel::channels::cline::Cline);
}

#[test]
#[cfg(feature = "kimi")]
fn kimi_refresh_requires_renewable_credentials() {
    check_mixed_credentials(&gproxy_channel::channels::kimi::Kimi);
}

#[test]
#[cfg(feature = "opencode")]
fn opencode_refresh_requires_renewable_zen_credentials() {
    use gproxy_channel::channel::CredentialRefresh;
    use gproxy_channel::channels::opencode::OpenCode;
    check_mixed_credentials(&OpenCode::ZEN);
    assert!(OpenCode::GO.credential_refresh().is_none());
    assert!(!OpenCode::GO.supports(&support::credential(
        "oauth",
        &json!({"refresh_token": "refresh"}),
        &json!({}),
    )));
}
