use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::json;

use crate::channel::ChannelError;

pub const CALLBACK_URL: &str = "https://tasklet.ai/oauth2callback";
const GOOGLE_CLIENT_ID: &str =
    "252828688609-4s8sdku4s84rlp4b6k1irb1fcf0aplhm.apps.googleusercontent.com";
const MICROSOFT_CLIENT_ID: &str = "da1ad0c1-c6e7-4311-815e-7ab56c5ffab4";

pub fn authorize_url(method: &str, csrf_token: &str) -> (String, String) {
    let (base, client_id, scope) = if method == "google" {
        (
            "https://accounts.google.com/o/oauth2/v2/auth",
            GOOGLE_CLIENT_ID,
            "email profile",
        )
    } else {
        (
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            MICROSOFT_CLIENT_ID,
            "openid profile email",
        )
    };
    let state = BASE64.encode(json!({"csrfToken":csrf_token}).to_string());
    let pairs = [
        ("client_id", client_id),
        ("redirect_uri", CALLBACK_URL),
        ("scope", scope),
        ("response_type", "code"),
        ("prompt", "select_account"),
        ("state", state.as_str()),
    ];
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    (format!("{base}?{query}"), state)
}

pub fn callback_code(callback: &str, expected_state: &str) -> Result<String, ChannelError> {
    let uri: http::Uri = callback
        .parse()
        .map_err(|_| ChannelError::Build("invalid Tasklet callback URL".into()))?;
    if uri.scheme_str() != Some("https")
        || uri.host() != Some("tasklet.ai")
        || uri.path() != "/oauth2callback"
    {
        return Err(ChannelError::Build("invalid Tasklet callback URL".into()));
    }
    let query = uri
        .query()
        .ok_or_else(|| ChannelError::Build("Tasklet callback is missing query data".into()))?;
    let state = query_param(query, "state")
        .ok_or_else(|| ChannelError::Build("Tasklet callback is missing state".into()))?;
    if state != expected_state {
        return Err(ChannelError::Build(
            "Tasklet callback state does not match".into(),
        ));
    }
    query_param(query, "code")
        .filter(|code| !code.is_empty())
        .ok_or_else(|| ChannelError::Build("Tasklet callback is missing code".into()))
}

fn query_param(query: &str, wanted: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == wanted).then(|| percent_decode(value)).flatten()
    })
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = hex(bytes[index + 1])?;
                let low = hex(bytes[index + 2])?;
                out.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
