//! The OAuth error codes, which are part of the protocol rather than of this
//! crate's error vocabulary.
//!
//! A client that gets `{"error":"invalid_grant"}` re-runs its login; one that
//! gets `{"error":"authorization_pending"}` waits and polls again; one that
//! gets `{"error":"invalid_request"}` has a bug. Those are different
//! behaviours, and flattening them all into [`AppError::Invalid`] would leave
//! the host inventing a code from an English message — which is how a client
//! ends up re-authenticating on a typo, or polling forever on a real failure.
//!
//! So the code travels with the error. Every issuer operation answers
//! [`AppError::OAuth`], carrying one [`IssuerErrorCode`] and a description
//! that is safe to return: these are protocol errors about the *request*, not
//! about the credential behind it, and RFC 6749 §5.2 defines
//! `error_description` as human-readable text for the developer of the client.

use crate::{AppError, dto::OAuthErrorBody};

/// The codes this issuer emits, from RFC 6749 §4.1.2.1 and §5.2, RFC 7009 §2.2
/// and RFC 8628 §3.5.
///
/// Exhaustive on purpose: a new failure has to choose one of these, because a
/// code a client has never heard of is worth no more than no code at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssuerErrorCode {
    /// Malformed, missing a required parameter, or repeating one.
    InvalidRequest,
    /// The client is unknown, disabled or retired.
    InvalidClient,
    /// The code, refresh token or device code is invalid, expired, revoked,
    /// already used, or belongs to another client.
    InvalidGrant,
    /// The client is registered but not allowed to use this grant type, or not
    /// allowed to this user by the client allowlist.
    UnauthorizedClient,
    /// The user refused, or the pending device authorization was denied.
    AccessDenied,
    UnsupportedGrantType,
    /// Anything but `code` at the authorization endpoint.
    UnsupportedResponseType,
    InvalidScope,
    /// This instance failed. The only code in the list that is not the
    /// client's fault.
    ServerError,
    /// RFC 8628 §3.5: the user has not decided yet. Keep polling.
    AuthorizationPending,
    /// RFC 8628 §3.5: polling faster than `interval`. Slow down and continue.
    SlowDown,
    /// RFC 8628 §3.5: the device code's lifetime is over. Start again.
    ExpiredToken,
}

impl IssuerErrorCode {
    /// The string on the wire. A contract: these may gain members, but an
    /// existing one must never change meaning.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidClient => "invalid_client",
            Self::InvalidGrant => "invalid_grant",
            Self::UnauthorizedClient => "unauthorized_client",
            Self::AccessDenied => "access_denied",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::UnsupportedResponseType => "unsupported_response_type",
            Self::InvalidScope => "invalid_scope",
            Self::ServerError => "server_error",
            Self::AuthorizationPending => "authorization_pending",
            Self::SlowDown => "slow_down",
            Self::ExpiredToken => "expired_token",
        }
    }

    /// The HTTP status a host answers with.
    ///
    /// `invalid_client` is **400 and not 401**. RFC 6749 §5.2 asks for 401
    /// only when the client tried to authenticate through an HTTP
    /// authentication scheme, which requires a `WWW-Authenticate` challenge in
    /// the response. Every client this registry holds is public — the
    /// discovery document says `token_endpoint_auth_methods_supported: ["none"]`
    /// — so there is no scheme to challenge and 400 is the correct answer.
    ///
    /// `access_denied` is 403 because the request was understood and refused,
    /// and the device-flow codes are 400 because RFC 8628 §3.5 defines them as
    /// token-endpoint errors, which RFC 6749 §5.2 puts at 400. A client that
    /// treated `authorization_pending` as a transport failure would stop
    /// polling.
    pub fn status_code(self) -> u16 {
        match self {
            Self::AccessDenied => 403,
            Self::ServerError => 500,
            _ => 400,
        }
    }
}

impl std::fmt::Display for IssuerErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One OAuth protocol failure: the code the client branches on, and the text
/// its developer reads.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{code}: {description}")]
pub struct IssuerError {
    pub code: IssuerErrorCode,
    pub description: String,
}

impl IssuerError {
    pub fn new(code: IssuerErrorCode, description: impl Into<String>) -> Self {
        Self {
            code,
            description: description.into(),
        }
    }

    /// The body a host serializes.
    pub fn body(&self) -> OAuthErrorBody {
        OAuthErrorBody {
            error: self.code.as_str().to_owned(),
            error_description: self.description.clone(),
        }
    }

    pub fn status_code(&self) -> u16 {
        self.code.status_code()
    }
}

/// One constructor per code, so a call site names the protocol error rather
/// than assembling it.
macro_rules! constructors {
    ($($name:ident => $code:ident,)*) => {
        impl IssuerError {
            $(
                pub fn $name(description: impl Into<String>) -> AppError {
                    AppError::OAuth(Self::new(IssuerErrorCode::$code, description))
                }
            )*
        }
    };
}

constructors! {
    invalid_request => InvalidRequest,
    invalid_client => InvalidClient,
    invalid_grant => InvalidGrant,
    unauthorized_client => UnauthorizedClient,
    access_denied => AccessDenied,
    unsupported_grant_type => UnsupportedGrantType,
    unsupported_response_type => UnsupportedResponseType,
    invalid_scope => InvalidScope,
    server_error => ServerError,
    authorization_pending => AuthorizationPending,
    slow_down => SlowDown,
    expired_token => ExpiredToken,
}

/// The RFC error body for **any** failure an issuer operation raised.
///
/// An issuer operation answers [`AppError::OAuth`] for everything it decided
/// itself, but a database that is down surfaces as [`AppError::Store`], and an
/// OAuth client must still be told in the protocol's own vocabulary. This is
/// the one mapping, so both hosts answer identically:
///
/// - an `AppError::OAuth` keeps its code and description;
/// - anything the instance is responsible for (5xx) becomes `server_error`
///   with a fixed description — the underlying message goes to the operator's
///   log, never to the client, because it can quote request values;
/// - any other refusal becomes `invalid_request`.
pub fn error_body(error: &AppError) -> OAuthErrorBody {
    match error {
        AppError::OAuth(issuer) => issuer.body(),
        other if other.status_code() >= 500 => OAuthErrorBody {
            error: IssuerErrorCode::ServerError.as_str().to_owned(),
            error_description: "the authorization server failed to process the request".to_owned(),
        },
        other => OAuthErrorBody {
            error: IssuerErrorCode::InvalidRequest.as_str().to_owned(),
            error_description: other.code().to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reaches_the_client_as_itself() {
        let error = IssuerError::invalid_grant("the code was already used");
        assert_eq!(error.code(), "invalid_grant");
        assert_eq!(error.status_code(), 400);
        let body = error_body(&error);
        assert_eq!(body.error, "invalid_grant");
        assert_eq!(body.error_description, "the code was already used");
    }

    #[test]
    fn a_public_clients_refusal_is_400_and_a_users_is_403() {
        assert_eq!(IssuerError::invalid_client("gone").status_code(), 400);
        assert_eq!(IssuerError::access_denied("refused").status_code(), 403);
        assert_eq!(IssuerError::server_error("boom").status_code(), 500);
    }

    #[test]
    fn the_device_codes_are_token_endpoint_errors_and_stay_pollable() {
        // 400, not 5xx: a client that saw a transport failure would stop.
        for error in [
            IssuerError::authorization_pending("waiting"),
            IssuerError::slow_down("too fast"),
            IssuerError::expired_token("over"),
        ] {
            assert_eq!(error.status_code(), 400, "{error}");
        }
        assert_eq!(
            IssuerError::authorization_pending("waiting").code(),
            "authorization_pending"
        );
    }

    #[test]
    fn a_failure_of_this_instance_never_quotes_itself_to_the_client() {
        let error = AppError::internal("connection to 10.0.0.4 refused: password `hunter2`");
        let body = error_body(&error);
        assert_eq!(body.error, "server_error");
        assert!(!body.error_description.contains("hunter2"));
    }

    #[test]
    fn a_non_oauth_refusal_still_answers_in_the_protocols_vocabulary() {
        let body = error_body(&AppError::not_found("oauth client", "cli"));
        assert_eq!(body.error, "invalid_request");
    }
}
