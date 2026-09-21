//! The OAuth issuer's wire shapes: what a downstream client sends to the
//! `/oauth/*` endpoints and what it gets back.
//!
//! **These are the only shapes in this module that are not camelCase.** Every
//! other family here answers a console this repository also owns, so it
//! follows the sdk's conventions. These answer `curl`, a Go CLI and somebody
//! else's editor extension, all of which implement RFC 6749, RFC 7009,
//! RFC 8414 and RFC 8628. Those documents fix the field names —
//! `access_token`, `client_id`, `error_description` — and renaming them would
//! not be a style choice but a different, non-interoperable protocol. The unit
//! convention changes with them: `expires_in` and `interval` are **seconds**,
//! because that is what the RFCs say, where the rest of this crate counts
//! milliseconds.
//!
//! **Nothing here holds a stored secret.** [`AuthorizationIssued::code`],
//! [`TokenResponse::access_token`], [`TokenResponse::refresh_token`] and
//! [`DeviceCodeResponse::device_code`] are plaintext as it is minted, returned
//! in the single response that creates it and never again; the rows behind
//! them keep only a SHA-256. See [`crate::operations::issuer`].

use serde::{Deserialize, Serialize};

/// The `token_type` every response carries. RFC 6750 spells it this way and
/// clients compare it case-insensitively, but not all of them do.
pub const TOKEN_TYPE: &str = "Bearer";

/// RFC 8628 §3.4's grant type for the device flow.
pub const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// RFC 6749 §4.1.1's authorization request.
///
/// Every field is `#[serde(default)]` so a request that omits one is answered
/// with `invalid_request` and a description, rather than with a deserialization
/// failure the client cannot act on.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AuthorizeQuery {
    /// Must be `code`. This issuer has no implicit and no hybrid flow.
    #[serde(default)]
    pub response_type: String,
    #[serde(default)]
    pub client_id: String,
    /// Matched **exactly** against `oauth_clients.redirect_uris`.
    #[serde(default)]
    pub redirect_uri: String,
    /// Space-delimited, per RFC 6749 §3.3. Empty asks for no scope.
    #[serde(default)]
    pub scope: String,
    /// Opaque client value, echoed back untouched on both outcomes. It is the
    /// client's CSRF defence and this issuer never interprets it.
    #[serde(default)]
    pub state: Option<String>,
    /// The base64url SHA-256 of the client's verifier (RFC 7636 §4.2).
    #[serde(default)]
    pub code_challenge: String,
    /// Must be `S256`.
    #[serde(default)]
    pub code_challenge_method: String,
}

/// What a consent screen renders, once the request has been found valid.
///
/// Returned by [`Issuer::authorize_details`](crate::operations::issuer::Issuer::authorize_details),
/// which validates everything an approval would validate. A screen built from
/// this can therefore show the user a decision that will actually be honoured
/// rather than one that fails after they take it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AuthorizeDetails {
    pub client_id: String,
    /// `oauth_clients.name`, which is what the user recognises. The id is a
    /// string a binary was compiled with and means nothing to them.
    pub client_name: String,
    /// The requested scopes, already split on whitespace and deduplicated.
    pub scopes: Vec<String>,
    /// The exact registered redirect URI the code would be delivered to.
    pub redirect_uri: String,
    pub state: Option<String>,
    /// The user a consent would bind the grant to.
    pub user_id: String,
}

/// What the person at the consent screen chose.
///
/// Named `ConsentDecision` rather than `Decision` because
/// [`snapshot::Decision`](crate::snapshot::Decision) is the permission
/// evaluator's verdict, and the two appear in the same files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
pub enum ConsentDecision {
    Approve,
    Deny,
}

/// The result of a consent decision: in both directions, a redirect back to
/// the client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case", tag = "outcome")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
pub enum AuthorizeOutcome {
    Issued(AuthorizationIssued),
    Denied(AuthorizationDenied),
}

impl AuthorizeOutcome {
    /// The absolute URL a host sends the browser to.
    pub fn location(&self) -> String {
        match self {
            Self::Issued(issued) => issued.location(),
            Self::Denied(denied) => denied.location(),
        }
    }

    /// The issued code, when the decision was an approval.
    pub fn issued(&self) -> Option<&AuthorizationIssued> {
        match self {
            Self::Issued(issued) => Some(issued),
            Self::Denied(_) => None,
        }
    }
}

/// An approved authorization. `code` is plaintext and exists only here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AuthorizationIssued {
    pub code: String,
    pub redirect_uri: String,
    pub state: Option<String>,
}

impl AuthorizationIssued {
    pub fn location(&self) -> String {
        let mut parameters = vec![("code", self.code.as_str())];
        if let Some(state) = &self.state {
            parameters.push(("state", state));
        }
        redirect(&self.redirect_uri, &parameters)
    }
}

/// A refused authorization, as RFC 6749 §4.1.2.1's error redirect.
///
/// The refusal goes to the client's registered redirect URI rather than to the
/// browser as an error page, because the client is what has to learn the
/// outcome — and it only ever goes there once the redirect URI has been proven
/// registered. A request whose `client_id` or `redirect_uri` is itself invalid
/// is refused before any of this exists, so a forged redirect target cannot be
/// used to bounce an error (and a `state`) to an attacker.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AuthorizationDenied {
    pub redirect_uri: String,
    pub state: Option<String>,
    /// Usually `access_denied`.
    pub error: String,
    pub error_description: String,
}

impl AuthorizationDenied {
    pub fn location(&self) -> String {
        let mut parameters = vec![
            ("error", self.error.as_str()),
            ("error_description", self.error_description.as_str()),
        ];
        if let Some(state) = &self.state {
            parameters.push(("state", state));
        }
        redirect(&self.redirect_uri, &parameters)
    }
}

/// RFC 6749 §4.1.3 / §6 and RFC 8628 §3.4, in one shape.
///
/// Which fields must be present depends on `grant_type`, so all of them are
/// optional here and the issuer names what is missing. That is deliberate: a
/// client that sends `code` with `grant_type=refresh_token` deserves
/// `invalid_request: refresh_token is required`, not a parse error listing
/// every field of every grant type.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokenRequest {
    /// `authorization_code`, `refresh_token` or [`DEVICE_GRANT_TYPE`].
    #[serde(default)]
    pub grant_type: String,
    /// Required for every grant type. These are public clients — there is no
    /// client secret to authenticate with — so this names the client and is
    /// checked against the grant, never trusted on its own.
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub device_code: Option<String>,
}

/// RFC 6749 §5.1. Both token values are plaintext and exist only here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokenResponse {
    pub access_token: String,
    /// Always [`TOKEN_TYPE`].
    pub token_type: String,
    /// The access token's remaining lifetime, in **seconds**.
    pub expires_in: i64,
    /// Rotated on every exchange: the one presented is consumed and this one
    /// replaces it. Presenting a consumed refresh token revokes the whole
    /// grant — see [`crate::operations::issuer`].
    pub refresh_token: String,
    /// The granted scopes, space-delimited, as they are recorded on the grant.
    pub scope: String,
}

/// RFC 8628 §3.1.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeviceCodeRequest {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub scope: String,
}

/// RFC 8628 §3.2. `device_code` is plaintext and exists only here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeviceCodeResponse {
    /// The secret the device polls with.
    pub device_code: String,
    /// The short code the person types, in the grouped form they read off the
    /// screen. Lookups normalize it, so any spelling with the right characters
    /// resolves.
    pub user_code: String,
    pub verification_uri: String,
    /// The same page with the code already filled in, for a device that can
    /// show a QR code (RFC 8628 §3.3.1).
    pub verification_uri_complete: String,
    /// Seconds.
    pub expires_in: i64,
    /// The minimum seconds between polls.
    pub interval: i64,
}

/// What the approval page renders for a pending device authorization.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeviceDetails {
    pub client_id: String,
    pub client_name: String,
    /// The normalized code, as stored.
    pub user_code: String,
    pub scopes: Vec<String>,
    /// Milliseconds, unlike the RFC fields above: this one is for a page this
    /// repository renders, not for the device.
    pub expires_at_ms: i64,
}

/// The outcome of an approval-page decision or a cancellation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeviceDecided {
    pub user_code: String,
    pub approved: bool,
}

/// RFC 7009 §2.1.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RevokeRequest {
    #[serde(default)]
    pub token: String,
    /// `access_token` or `refresh_token`. A hint only: RFC 7009 §2.1 requires
    /// the server to search the other kind anyway, so this is not used to
    /// narrow anything.
    #[serde(default)]
    pub token_type_hint: Option<String>,
    /// When present, the token must belong to this client. A public client has
    /// no secret to prove with, so this cannot authorize a revocation — it can
    /// only refuse one, which is the direction that matters.
    #[serde(default)]
    pub client_id: Option<String>,
}

/// RFC 8414 §2, restricted to what this issuer actually implements.
///
/// v3 never served this document, so every client had its endpoints compiled
/// in. Publishing it is what lets a client discover the mount it was pointed
/// at instead of assuming one.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AuthorizationServerMetadata {
    /// The issuer identifier: exactly the mount this document was fetched
    /// from, with no trailing slash (RFC 8414 §2).
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    /// RFC 8628 §4.
    pub device_authorization_endpoint: String,
    /// RFC 7009 §2.
    pub revocation_endpoint: String,
    /// `["code"]`.
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    /// `["S256"]`. `plain` is deliberately absent.
    pub code_challenge_methods_supported: Vec<String>,
    /// `["none"]`: every registered client is public and this registry issues
    /// no client secrets.
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// `["none"]` as well, for the revocation endpoint.
    pub revocation_endpoint_auth_methods_supported: Vec<String>,
}

/// RFC 6749 §5.2's error body, which RFC 7009 and RFC 8628 reuse.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OAuthErrorBody {
    pub error: String,
    pub error_description: String,
}

/// `base` with `parameters` appended to its query string.
///
/// The separator is chosen by looking for an existing `?`, because RFC 6749
/// §3.1.2 permits a registered redirect URI to carry a query component of its
/// own and appending a second `?` would silently corrupt it.
fn redirect(base: &str, parameters: &[(&str, &str)]) -> String {
    let mut out = base.to_owned();
    let mut separator = if base.contains('?') { '&' } else { '?' };
    for (name, value) in parameters {
        out.push(separator);
        out.push_str(name);
        out.push('=');
        out.push_str(&encode(value));
        separator = '&';
    }
    out
}

/// Percent-encode everything outside RFC 3986's unreserved set.
///
/// Written out rather than taken from `form_urlencoded`, which this crate does
/// depend on: its `byte_serialize` implements
/// `application/x-www-form-urlencoded`, where a space becomes `+`. That is
/// right for a POST body and ambiguous in a URI query component, which is what
/// this builds — a client that reads the redirect as a URI rather than as a
/// form would hand its `state` back with a literal plus in it.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unreserved_bytes_survive_encoding() {
        assert_eq!(encode("abcXYZ019-._~"), "abcXYZ019-._~");
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
        // Multi-byte characters are encoded per UTF-8 byte.
        assert_eq!(encode("é"), "%C3%A9");
    }

    #[test]
    fn a_redirect_keeps_the_uris_own_query_string() {
        assert_eq!(
            redirect("https://app.example/cb", &[("code", "abc")]),
            "https://app.example/cb?code=abc"
        );
        assert_eq!(
            redirect("https://app.example/cb?tenant=acme", &[("code", "abc")]),
            "https://app.example/cb?tenant=acme&code=abc"
        );
    }

    #[test]
    fn an_issued_code_and_a_denial_both_echo_the_state_verbatim() {
        let issued = AuthorizationIssued {
            code: "c+1".into(),
            redirect_uri: "http://127.0.0.1:1455/cb".into(),
            state: Some("s t".into()),
        };
        assert_eq!(
            issued.location(),
            "http://127.0.0.1:1455/cb?code=c%2B1&state=s%20t"
        );
        let denied = AuthorizationDenied {
            redirect_uri: "http://127.0.0.1:1455/cb".into(),
            state: Some("s t".into()),
            error: "access_denied".into(),
            error_description: "the user refused".into(),
        };
        assert_eq!(
            denied.location(),
            "http://127.0.0.1:1455/cb?error=access_denied\
             &error_description=the%20user%20refused&state=s%20t"
        );
    }

    #[test]
    fn a_request_without_state_does_not_invent_one() {
        let issued = AuthorizationIssued {
            code: "abc".into(),
            redirect_uri: "http://127.0.0.1:1455/cb".into(),
            state: None,
        };
        assert_eq!(issued.location(), "http://127.0.0.1:1455/cb?code=abc");
    }

    #[test]
    fn the_authorize_query_tolerates_every_missing_field() {
        // A client that sends nothing gets a named refusal from the issuer,
        // not a serde error it cannot read.
        let query: AuthorizeQuery = serde_json::from_str("{}").unwrap();
        assert!(query.response_type.is_empty());
        assert!(query.client_id.is_empty());
        assert_eq!(query.state, None);
        let request: TokenRequest = serde_json::from_str("{}").unwrap();
        assert!(request.grant_type.is_empty());
        assert_eq!(request.code, None);
    }

    #[test]
    fn the_wire_names_are_the_rfcs_and_not_this_crates() {
        let response = TokenResponse {
            access_token: "a".into(),
            token_type: TOKEN_TYPE.into(),
            expires_in: 3600,
            refresh_token: "r".into(),
            scope: "openid".into(),
        };
        let text = serde_json::to_string(&response).unwrap();
        assert!(text.contains("\"access_token\""), "{text}");
        assert!(text.contains("\"refresh_token\""), "{text}");
        assert!(!text.contains("accessToken"), "{text}");
    }
}
