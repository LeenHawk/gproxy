//! `ProtectedResourceOAuth` against an authorization server that lives in
//! this process: every exchange is a real HTTP request to an `OutboundClient`.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

use base64::Engine as _;
use gproxy_client::OutboundClient;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use gproxy_upstream::{
    AcquiredCredential, AuthorizationCode, AuthorizationRequest, CredentialContext,
    CredentialRefresh, CredentialView, LoginContext, OAuthAuthorizationCode, OAuthCredential,
    ProviderView, UpstreamError,
    protected_resource::{ProtectedResourceOAuth, discover},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RESOURCE: &str = "https://tools.example/mcp";
const ISSUER: &str = "https://auth.example";
const REDIRECT: &str = "https://gateway.example/callback";
const LOOPBACK: &str = "http://127.0.0.1:47821/callback";
const OAUTH: ProtectedResourceOAuth = ProtectedResourceOAuth {
    client_name: "gproxy",
    redirect_uri: LOOPBACK,
};

#[derive(Default)]
struct Server {
    /// Publishes no metadata at all.
    undiscoverable: bool,
    refuses_registration: bool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    requests: Vec<String>,
    /// What the registered client may be sent back to.
    redirect_uris: Value,
    /// Authorization code → the challenge it was issued for.
    codes: HashMap<String, String>,
    refresh_tokens: Vec<String>,
    issued: u32,
}

fn reply(status: StatusCode, body: Value) -> WireResponse<HttpBody> {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}

fn refused(error: &str) -> WireResponse<HttpBody> {
    reply(StatusCode::BAD_REQUEST, json!({ "error": error }))
}

impl Server {
    /// The person approving in a browser: the authorization endpoint's part.
    fn approve(&self, authorize_url: &str) -> String {
        let url = url::Url::parse(authorize_url).unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["resource"], RESOURCE);
        assert_eq!(query["client_id"], "client-1");
        assert_eq!(
            json!([query["redirect_uri"]]),
            self.state.lock().unwrap().redirect_uris
        );
        assert_eq!(query["scope"], "tools.read offline_access");
        let code = "code-1".to_owned();
        self.state
            .lock()
            .unwrap()
            .codes
            .insert(code.clone(), query["code_challenge"].clone());
        code
    }

    fn token(&self, form: &HashMap<String, String>) -> WireResponse<HttpBody> {
        let mut state = self.state.lock().unwrap();
        if form.get("client_id").map(String::as_str) != Some("client-1")
            || form.get("resource").map(String::as_str) != Some(RESOURCE)
        {
            return refused("invalid_client");
        }
        match form["grant_type"].as_str() {
            "authorization_code" => {
                let Some(challenge) = state.codes.remove(&form["code"]) else {
                    return refused("invalid_grant");
                };
                let verifier = Sha256::digest(form["code_verifier"].as_bytes());
                if base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier) != challenge {
                    return refused("invalid_grant");
                }
            }
            "refresh_token" => {
                let presented = &form["refresh_token"];
                let Some(at) = state.refresh_tokens.iter().position(|t| t == presented) else {
                    return refused("invalid_grant");
                };
                state.refresh_tokens.remove(at);
            }
            _ => return refused("unsupported_grant_type"),
        }
        state.issued += 1;
        let refresh = format!("refresh-{}", state.issued);
        state.refresh_tokens.push(refresh.clone());
        reply(
            StatusCode::OK,
            json!({
                "access_token": format!("access-{}", state.issued),
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": refresh,
                "scope": "tools.read",
            }),
        )
    }
}

impl OutboundClient for Server {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let HttpBody::Bytes(body) = body else {
                unreachable!("requests are buffered")
            };
            let url = parts.uri.to_string();
            self.state
                .lock()
                .unwrap()
                .requests
                .push(format!("{} {url}", parts.method));
            let not_found = reply(StatusCode::NOT_FOUND, json!({}));
            if self.undiscoverable && url.contains("/.well-known/") {
                return Ok(not_found);
            }
            Ok(match (parts.method, url.as_str()) {
                (Method::GET, RESOURCE) => {
                    let mut challenge = reply(StatusCode::UNAUTHORIZED, json!({}));
                    challenge.headers.insert(
                        header::WWW_AUTHENTICATE,
                        HeaderValue::from_static(
                            r#"Bearer resource_metadata="https://tools.example/meta/resource""#,
                        ),
                    );
                    challenge
                }
                (Method::GET, "https://tools.example/meta/resource") => reply(
                    StatusCode::OK,
                    json!({
                        "resource": RESOURCE,
                        "authorization_servers": [ISSUER],
                        "scopes_supported": ["tools.read"],
                    }),
                ),
                (Method::GET, "https://auth.example/.well-known/oauth-authorization-server") => {
                    reply(
                        StatusCode::OK,
                        json!({
                            "issuer": ISSUER,
                            "authorization_endpoint": "https://auth.example/authorize",
                            "token_endpoint": "https://auth.example/token",
                            "registration_endpoint": "https://auth.example/register",
                            "scopes_supported": ["tools.read", "offline_access"],
                            "code_challenge_methods_supported": ["S256"],
                        }),
                    )
                }
                (Method::POST, "https://auth.example/register") => {
                    let asked: Value = serde_json::from_slice(&body).unwrap();
                    if self.refuses_registration {
                        reply(StatusCode::FORBIDDEN, json!({ "error": "access_denied" }))
                    } else {
                        self.state.lock().unwrap().redirect_uris = asked["redirect_uris"].clone();
                        reply(StatusCode::CREATED, json!({ "client_id": "client-1" }))
                    }
                }
                (Method::POST, "https://auth.example/token") => {
                    self.token(&form_urlencoded::parse(&body).into_owned().collect())
                }
                _ => not_found,
            })
        })
    }
}

fn login<'a>(server: &'a Server, config: &'a Value) -> LoginContext<'a> {
    LoginContext {
        provider: ProviderView {
            id: "tools",
            channel: "mcp",
            base_url: Some(RESOURCE),
            config,
        },
        client: server,
        variant: None,
    }
}

fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn sign_in(
    server: &Server,
    verifier_at_exchange: &str,
) -> Result<OAuthCredential, UpstreamError> {
    let config = Value::Null;
    let code_challenge = challenge("the-verifier");
    let start = OAUTH
        .authorize(
            login(server, &config),
            AuthorizationRequest {
                redirect_uri: REDIRECT,
                state: "state-1",
                code_challenge: &code_challenge,
            },
        )
        .await?;
    let code = server.approve(&start.authorize_url);
    OAUTH
        .exchange(
            login(server, &config),
            AuthorizationCode {
                code: &code,
                redirect_uri: &start.redirect_uri,
                code_verifier: verifier_at_exchange,
                state: "state-1",
                provider_state: &start.provider_state,
            },
        )
        .await
}

async fn refresh(server: &Server, secret: &Value) -> Result<Value, UpstreamError> {
    let config = Value::Null;
    let metadata = Value::Null;
    let credential = CredentialView {
        id: "credential-1",
        provider_id: "tools",
        auth_kind: "oauth",
        secret,
        metadata: &metadata,
        version: 1,
        expires_at_ms: None,
    };
    assert!(OAUTH.supports(&credential));
    let update = OAUTH
        .refresh(CredentialContext {
            provider: login(server, &config).provider,
            credential,
            client: server,
        })
        .await?;
    assert!(update.expires_at_ms.is_some());
    Ok(update.secret)
}

#[tokio::test]
async fn sign_in_registers_a_client_and_keeps_it_out_of_the_metadata() {
    let server = Server::default();
    let credential = sign_in(&server, "the-verifier").await.unwrap();
    assert_eq!(credential.access_token, "access-1");
    assert_eq!(credential.refresh_token.as_deref(), Some("refresh-1"));
    assert_eq!(credential.scopes, ["tools.read"]);
    assert!(credential.expires_at_ms.is_some());
    assert_eq!(
        credential.provider_secrets,
        BTreeMap::from([
            ("client_id".to_owned(), json!("client-1")),
            ("resource".to_owned(), json!(RESOURCE)),
            (
                "token_endpoint".to_owned(),
                json!("https://auth.example/token")
            ),
            ("token_endpoint_auth_method".to_owned(), json!("none")),
        ])
    );
    assert_eq!(AcquiredCredential::from(credential).metadata, json!({}));
}

#[tokio::test]
async fn a_host_that_names_no_redirect_is_given_the_loopback_one() {
    let server = Server::default();
    let config = Value::Null;
    let code_challenge = challenge("the-verifier");
    let start = OAUTH
        .authorize(
            login(&server, &config),
            AuthorizationRequest {
                redirect_uri: "",
                state: "state-1",
                code_challenge: &code_challenge,
            },
        )
        .await
        .unwrap();
    assert_eq!(start.redirect_uri, LOOPBACK);
    assert_eq!(
        server.state.lock().unwrap().redirect_uris,
        json!([LOOPBACK])
    );
    server.approve(&start.authorize_url);
}

#[tokio::test]
async fn a_resource_without_metadata_falls_back_to_its_origin_and_then_fails() {
    let server = Server {
        undiscoverable: true,
        ..Server::default()
    };
    let error = discover(&server, RESOURCE, None).await.unwrap_err();
    assert!(
        matches!(&error, UpstreamError::InvalidResponse(message) if message.contains("https://tools.example")),
        "{error}"
    );
    let requests = server.state.lock().unwrap().requests.clone();
    assert_eq!(
        requests,
        [
            "GET https://tools.example/.well-known/oauth-protected-resource/mcp",
            "GET https://tools.example/.well-known/oauth-protected-resource",
            "GET https://tools.example/.well-known/oauth-authorization-server",
            "GET https://tools.example/.well-known/openid-configuration",
        ]
    );
}

#[tokio::test]
async fn a_refused_registration_fails_the_sign_in_with_the_servers_answer() {
    let server = Server {
        refuses_registration: true,
        ..Server::default()
    };
    let error = sign_in(&server, "the-verifier").await.err().unwrap();
    assert!(
        matches!(error, UpstreamError::UpstreamResponse { status, .. } if status == StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn a_verifier_that_does_not_match_the_challenge_is_refused() {
    let server = Server::default();
    let error = sign_in(&server, "another-verifier").await.err().unwrap();
    assert!(
        matches!(error, UpstreamError::UpstreamResponse { status, .. } if status == StatusCode::BAD_REQUEST)
    );
}

#[tokio::test]
async fn refresh_rotates_the_token_and_a_spent_one_is_rejected_for_good() {
    let server = Server::default();
    let first = AcquiredCredential::from(sign_in(&server, "the-verifier").await.unwrap()).secret;

    let second = refresh(&server, &first).await.unwrap();
    assert_eq!(second["access_token"], "access-2");
    assert_eq!(second["refresh_token"], "refresh-2");
    assert_eq!(second["provider_secrets"], first["provider_secrets"]);

    // The server rotated: the first refresh token is gone.
    let error = refresh(&server, &first).await.unwrap_err();
    assert!(
        matches!(&error, UpstreamError::RefreshRejected(reason) if reason == "invalid_grant"),
        "{error}"
    );
}
