#![cfg(not(target_arch = "wasm32"))]
//! The OAuth issuer over HTTP: the full authorization-code flow through the
//! router, the discovery document at both of its URLs, and the envelope that
//! must not be this crate's other one.
//!
//! The protocol itself is tested exhaustively in `gproxy-app`'s `tests/oauth`.
//! What is asserted here is the binding: the form encoding an RFC-conforming
//! client actually sends, the mount the issuer identifier is taken from, the
//! `Cache-Control` a token response must carry, and the error document.

mod support;

use gproxy_app::{
    AppConfig,
    dto::{AuthorizeOutcome, ConsentDecision, OAuthClientWrite, UserWrite},
};
use http::StatusCode;
use serde_json::json;
use support::{Host, form, get, post, with};

/// RFC 7636 appendix B's verifier and its S256 challenge, so the PKCE check
/// runs against the specification's own vector.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const REDIRECT: &str = "http://127.0.0.1:1455/auth/callback";
const PASSWORD: &str = "correct horse battery";

/// One person with a password, and one registered public client.
async fn instance() -> Host {
    instance_as(None).await
}

/// [`instance`], with `alice` holding `role`.
async fn instance_as(role: Option<&str>) -> Host {
    let host = Host::new().await;
    let data = host.data();
    let operations = host.operations(&data);
    operations
        .users()
        .create(UserWrite {
            name: "alice".into(),
            password: Some(PASSWORD.into()),
            role: role.map(str::to_owned),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    operations
        .oauth_clients()
        .create(OAuthClientWrite {
            id: "cli-app".into(),
            name: "CLI".into(),
            redirect_uris: vec![REDIRECT.into()],
            ..OAuthClientWrite::default()
        })
        .await
        .unwrap();
    drop(data);
    host.publish().await;
    host
}

fn authorize_query() -> String {
    serde_urlencoded::to_string([
        ("response_type", "code"),
        ("client_id", "cli-app"),
        ("redirect_uri", REDIRECT),
        ("scope", "openid profile"),
        ("state", "xyz"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
    ])
    .unwrap()
}

/// Sign in and answer the session cookie, which is what a consent page holds.
async fn sign_in(host: &Host) -> String {
    let login = host
        .send(post(
            "/portal/api/login",
            json!({ "name": "alice", "password": PASSWORD }),
        ))
        .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    format!("gproxy_session={}", login.json()["token"].as_str().unwrap())
}

/// Approve [`authorize_query`] with `cookie` and redeem the code: the access
/// token a client ends up holding.
async fn access_token(host: &Host, cookie: &str) -> String {
    let approved = host
        .send(with(
            with(
                post(
                    &format!("/v1/oauth/authorize?{}", authorize_query()),
                    json!({ "decision": ConsentDecision::Approve }),
                ),
                "cookie",
                cookie,
            ),
            "origin",
            "http://gproxy.local",
        ))
        .await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    let outcome: AuthorizeOutcome =
        serde_json::from_value(approved.json()["outcome"].clone()).unwrap();
    let code = outcome.issued().expect("approved").code.clone();
    let token = host
        .send(form(
            "/v1/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("client_id", "cli-app"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
            ],
        ))
        .await;
    assert_eq!(token.status, StatusCode::OK, "{}", token.text());
    token.json()["access_token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn an_administrators_token_is_not_an_administrative_credential() {
    let host = instance_as(Some("admin")).await;
    let cookie = sign_in(&host).await;
    let access = access_token(&host, &cookie).await;

    let users = host
        .send(support::keyed(get("/admin/api/users"), &access))
        .await;
    assert_eq!(users.status, StatusCode::FORBIDDEN, "{}", users.text());
    let export = host
        .send(support::keyed(get("/admin/api/export"), &access))
        .await;
    assert_eq!(export.status, StatusCode::FORBIDDEN, "{}", export.text());
}

#[tokio::test]
async fn only_a_signed_in_person_can_approve_a_grant() {
    let host = instance().await;
    let cookie = sign_in(&host).await;
    let access = access_token(&host, &cookie).await;
    let decide = || {
        post(
            &format!("/v1/oauth/authorize?{}", authorize_query()),
            json!({ "decision": ConsentDecision::Approve }),
        )
    };

    // A token cannot approve a fresh grant for itself.
    let answer = host.send(support::keyed(decide(), &access)).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{}", answer.text());
    // Nor can a cookie a foreign page made the browser attach.
    let answer = host.send(with(decide(), "cookie", &cookie)).await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.text());
    let answer = host
        .send(with(
            with(decide(), "cookie", &cookie),
            "origin",
            "https://evil.example",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.text());
}

#[tokio::test]
async fn the_full_code_flow_runs_over_http() {
    let host = instance().await;
    let cookie = sign_in(&host).await;
    let query = authorize_query();

    // The consent page fetches what it is about to show.
    let details = host
        .send(with(
            get(&format!("/v1/oauth/authorize?{query}")),
            "cookie",
            &cookie,
        ))
        .await;
    assert_eq!(details.status, StatusCode::OK, "{}", details.text());
    assert_eq!(details.json()["client_id"], "cli-app");
    assert_eq!(details.json()["redirect_uri"], REDIRECT);

    // The person approves. The answer names where to send the browser rather
    // than redirecting, because the caller is the page's own `fetch`.
    let approved = host
        .send(with(
            with(
                post(
                    &format!("/v1/oauth/authorize?{query}"),
                    json!({ "decision": ConsentDecision::Approve }),
                ),
                "cookie",
                &cookie,
            ),
            "origin",
            "http://gproxy.local",
        ))
        .await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    let location = approved.json()["location"].as_str().unwrap().to_owned();
    assert!(location.starts_with(REDIRECT), "{location}");
    assert!(location.contains("state=xyz"), "{location}");
    let outcome: AuthorizeOutcome =
        serde_json::from_value(approved.json()["outcome"].clone()).unwrap();
    let code = outcome.issued().expect("approved").code.clone();

    // The client redeems it with a form body, as RFC 6749 §4.1.3 requires.
    let token = host
        .send(form(
            "/v1/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("client_id", "cli-app"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
            ],
        ))
        .await;
    assert_eq!(token.status, StatusCode::OK, "{}", token.text());
    let access = token.json()["access_token"].as_str().unwrap().to_owned();
    let refresh = token.json()["refresh_token"].as_str().unwrap().to_owned();
    assert_eq!(token.json()["token_type"], "Bearer");
    assert!(
        token
            .header("cache-control")
            .unwrap_or_default()
            .contains("no-store"),
        "RFC 6749 §5.1: a token response is never cached"
    );

    // The access token authenticates as the person who granted it.
    let context = host
        .send(support::keyed(get("/portal/api/context"), &access))
        .await;
    assert_eq!(context.status, StatusCode::OK, "{}", context.text());
    assert_eq!(context.json()["user"]["name"], "alice");

    // A refresh exchanges for a new pair.
    let refreshed = host
        .send(form(
            "/v1/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("client_id", "cli-app"),
                ("refresh_token", &refresh),
            ],
        ))
        .await;
    assert_eq!(refreshed.status, StatusCode::OK, "{}", refreshed.text());
    assert_ne!(refreshed.json()["refresh_token"], refresh);

    // Replaying the spent refresh token kills the whole grant.
    let replayed = host
        .send(form(
            "/v1/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("client_id", "cli-app"),
                ("refresh_token", &refresh),
            ],
        ))
        .await;
    assert_eq!(replayed.status, StatusCode::BAD_REQUEST);
    assert_eq!(replayed.json()["error"], "invalid_grant");
    let context = host
        .send(support::keyed(get("/portal/api/context"), &access))
        .await;
    assert_eq!(
        context.status,
        StatusCode::UNAUTHORIZED,
        "a replay revokes the grant, the already-issued access token included"
    );
}

#[tokio::test]
async fn the_oauth_error_envelope_is_not_the_product_one() {
    let host = instance().await;

    // A code that was never minted.
    let answer = host
        .send(form(
            "/v1/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("client_id", "cli-app"),
                ("code", "not-a-code"),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
            ],
        ))
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    let body = answer.json();
    // RFC 6749 §5.2: `error` is a string with a code in it.
    assert_eq!(body["error"], "invalid_grant");
    assert!(body["error_description"].is_string(), "{body}");
    assert!(
        body["error"]["code"].is_null(),
        "an OAuth client cannot read the product envelope: {body}"
    );

    // The same instance, one path along, answers the other document.
    let answer = host.send(get("/admin/api/users")).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    let body = answer.json();
    assert_eq!(body["error"]["code"], "unauthorized");
    assert!(
        body["error_description"].is_null(),
        "the product envelope is not the OAuth one: {body}"
    );

    // An unregistered client is `invalid_client`, still in the RFC's shape.
    let answer = host
        .send(form(
            "/v1/oauth/device/code",
            &[("client_id", "nobody"), ("scope", "openid")],
        ))
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(answer.json()["error"], "invalid_client");
}

#[tokio::test]
async fn the_discovery_document_names_the_mount_it_was_fetched_from() {
    let host = Host::with_config(AppConfig {
        public_base_url: Some("https://gproxy.example.com".into()),
        ..AppConfig::default()
    })
    .await;
    host.publish().await;

    // Issuer-relative, on the aggregated mount.
    let answer = host
        .send(get("/v1/.well-known/oauth-authorization-server"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    let document = answer.json();
    assert_eq!(document["issuer"], "https://gproxy.example.com/v1");
    assert_eq!(
        document["token_endpoint"],
        "https://gproxy.example.com/v1/oauth/token"
    );
    assert_eq!(
        document["authorization_endpoint"],
        "https://gproxy.example.com/v1/oauth/authorize"
    );
    assert_eq!(
        document["code_challenge_methods_supported"],
        json!(["S256"])
    );

    // RFC 8414 §3.1's path-insertion form, which names its own mount and
    // therefore answers for a mount the request did not arrive on.
    let answer = host
        .send(get("/.well-known/oauth-authorization-server/acme/v1"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(
        answer.json()["issuer"],
        "https://gproxy.example.com/acme/v1"
    );
    assert_eq!(
        answer.json()["token_endpoint"],
        "https://gproxy.example.com/acme/v1/oauth/token"
    );
}

#[tokio::test]
async fn the_issuer_falls_back_to_the_requests_own_host_and_never_to_a_forged_scheme() {
    let host = instance().await;

    let answer = host
        .send(with(
            with(
                get("/v1/.well-known/oauth-authorization-server"),
                "host",
                "gproxy.local:7070",
            ),
            // An untrusted peer's claim about the scheme. The default
            // configuration trusts no proxy, so it is ignored.
            "x-forwarded-proto",
            "https",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["issuer"], "http://gproxy.local:7070/v1");
}

#[tokio::test]
async fn a_browser_navigating_to_authorize_is_sent_to_the_consent_page() {
    let host = instance().await;
    let query = authorize_query();
    let answer = host
        .send(with(
            get(&format!("/v1/oauth/authorize?{query}")),
            "accept",
            "text/html,application/xhtml+xml",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::FOUND);
    let location = answer.header("location").unwrap();
    assert!(location.starts_with("/console/authorize?"), "{location}");
    assert!(location.contains("client_id=cli-app"), "{location}");
}

#[tokio::test]
async fn an_unauthenticated_consent_fetch_is_refused_in_the_oauth_vocabulary() {
    let host = instance().await;
    let query = authorize_query();
    let answer = host
        .send(get(&format!("/v1/oauth/authorize?{query}")))
        .await;
    // The status is this crate's — 401, because no credential was presented —
    // while the body is the protocol's: `unauthorized` is not an RFC 6749
    // code, so it is reported as the closest one the protocol has rather than
    // leaking this crate's vocabulary into an OAuth client.
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(answer.json()["error"], "invalid_request");
    assert_eq!(answer.json()["error_description"], "unauthorized");
}

#[tokio::test]
async fn a_signed_in_person_approves_a_device_from_the_console() {
    let host = instance().await;
    let cookie = sign_in(&host).await;
    let started = host
        .send(form(
            "/v1/oauth/device/code",
            &[("client_id", "cli-app"), ("scope", "openid")],
        ))
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let started = started.json();
    let user_code = started["user_code"].as_str().unwrap().to_owned();
    let device_code = started["device_code"].as_str().unwrap().to_owned();
    // The page a person is sent to is the console's.
    assert!(
        started["verification_uri"]
            .as_str()
            .unwrap()
            .ends_with("/console/device"),
        "{started}"
    );
    let poll = || {
        form(
            "/v1/oauth/token",
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", "cli-app"),
                ("device_code", &device_code),
            ],
        )
    };
    assert_eq!(
        host.send(poll()).await.json()["error"],
        "authorization_pending"
    );

    let details = host
        .send(with(
            get(&format!("/portal/api/oauth/device?userCode={user_code}")),
            "cookie",
            &cookie,
        ))
        .await;
    assert_eq!(details.status, StatusCode::OK, "{}", details.text());
    assert_eq!(details.json()["client_id"], "cli-app");

    let decide = || {
        post(
            "/portal/api/oauth/device",
            json!({ "userCode": user_code, "decision": ConsentDecision::Approve }),
        )
    };
    // A token cannot approve a device, even one belonging to that person.
    let access = access_token(&host, &cookie).await;
    let refused = host.send(support::keyed(decide(), &access)).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());

    let decided = host
        .send(with(
            with(decide(), "cookie", &cookie),
            "origin",
            "http://gproxy.local",
        ))
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    assert_eq!(decided.json()["approved"], true);

    let token = host.send(poll()).await;
    assert_eq!(token.status, StatusCode::OK, "{}", token.text());
    assert!(token.json()["access_token"].is_string());
}
