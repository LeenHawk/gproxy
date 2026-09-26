#![cfg(not(target_arch = "wasm32"))]
//! `AdminScope` through the router: who may act as what, and what each scope
//! sees on the same routes.
//!
//! The suite is a truth table rather than a walk through anecdotes. Four
//! scopes — instance, organization, team and none — crossed with one
//! representative of each family group:
//!
//! | | instance machinery (`/providers`) | `/quotas` | `/credentials` | identity (`/users`) |
//! |---|---|---|---|---|
//! | instance | served | served, unfiltered | served, unfiltered | served |
//! | organization | `403` | filtered to the organization and its teams | filtered the same way | `403` |
//! | team | `403` | filtered to the team | filtered to the team | `403` |
//! | none | `403` before any route runs | `403` | `403` | `403` |
//!
//! Plus the three rules that are not a cell of that table: a row outside the
//! scope is `NotFound` and never `Forbidden`, a write naming an owner outside
//! the scope is refused, a scope header naming an organization the caller does
//! not administer is refused, and an API key's binding wins over any header it
//! sends.

mod support;

use gproxy_app::AppConfig;
use gproxy_store::entity::identity::membership_role::MembershipRole;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{Answer, Host, get, keyed, post, request, with};

const SCOPE: &str = "x-gproxy-admin-scope";

/// One instance with two organizations, one team each, and the six callers the
/// table above needs.
///
/// * `root` — instance administrator, key `k-root`;
/// * `orgadmin` — `admin` of `acme`, signs in with a password;
/// * `plain` — `member` of `acme`, signs in with a password;
/// * `lead` — `admin` of team `core` only;
/// * `both` — `admin` of `acme` **and** `globex`, so it has to name a scope;
/// * `k-acme` — an API key of `plain`'s bound to `acme`, and `k-core` bound to
///   team `core`: a member's keys, which administer nothing;
/// * `k-acme-admin` — `orgadmin`'s key bound to `acme`, and `k-core-lead`
///   `lead`'s bound to `core`.
///
/// Credentials and budgets exist for every owner shape: unowned, `acme`'s,
/// `core`'s (a team inside acme), `globex`'s, and `plain`'s own.
async fn instance() -> Host {
    let host = Host::with_config(AppConfig {
        cors_origins: vec!["https://console.example.com".into()],
        ..AppConfig::default()
    })
    .await;
    let handle = host.handle();

    support::organization(&handle, "acme").await;
    support::organization(&handle, "globex").await;
    support::team(&handle, "core", "acme").await;
    support::team(&handle, "ops", "globex").await;

    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, "k-root", "root", None, None).await;
    for person in ["orgadmin", "plain", "lead", "both"] {
        support::person(&handle, person, "user").await;
    }
    support::org_member(&handle, "acme", "orgadmin", MembershipRole::Admin).await;
    support::org_member(&handle, "acme", "plain", MembershipRole::Member).await;
    support::org_member(&handle, "acme", "both", MembershipRole::Admin).await;
    support::org_member(&handle, "globex", "both", MembershipRole::Admin).await;
    support::team_member(&handle, "core", "lead", MembershipRole::Admin).await;

    // Keys: one bound to the organization, one to the team, one to nothing.
    support::api_key(&handle, "k-acme", "plain", Some("acme"), None).await;
    support::api_key(&handle, "k-core", "plain", None, Some("core")).await;
    support::api_key(&handle, "k-loose", "plain", None, None).await;
    support::api_key(&handle, "k-acme-admin", "orgadmin", Some("acme"), None).await;
    support::api_key(&handle, "k-core-lead", "lead", None, Some("core")).await;

    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c-shared", "p1", None, None, None).await;
    support::credential(&handle, "c-acme", "p1", None, None, Some("acme")).await;
    support::credential(&handle, "c-core", "p1", None, Some("core"), None).await;
    support::credential(&handle, "c-globex", "p1", None, None, Some("globex")).await;
    support::credential(&handle, "c-plain", "p1", Some("plain"), None, None).await;

    support::spent_budget(&handle, "q-acme", "org", "acme").await;
    support::spent_budget(&handle, "q-core", "team", "core").await;
    support::spent_budget(&handle, "q-globex", "org", "globex").await;
    support::spent_budget(&handle, "q-user", "user", "plain").await;
    support::spent_budget(&handle, "q-limit", "credential", "c-shared").await;
    host.publish().await;

    let data = host.data();
    for person in ["orgadmin", "plain", "lead", "both"] {
        host.operations(&data)
            .users()
            .set_password(person, "correct horse battery")
            .await
            .unwrap();
    }
    drop(data);
    host.publish().await;
    host
}

/// A session cookie for one person, through the real portal login.
async fn cookie(host: &Host, user: &str) -> String {
    let login = host
        .send(post(
            "/portal/api/login",
            json!({ "name": user, "password": "correct horse battery" }),
        ))
        .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    format!("gproxy_session={}", login.json()["token"].as_str().unwrap())
}

/// A `GET` as a cookie session, optionally naming a scope.
async fn as_session(host: &Host, cookie: &str, scope: Option<&str>, uri: &str) -> Answer {
    let mut request = with(get(uri), "cookie", cookie);
    if let Some(scope) = scope {
        request = with(request, SCOPE, scope);
    }
    host.send(request).await
}

/// The `id` of every item of a page answer, sorted.
fn ids(answer: &Answer) -> Vec<String> {
    let mut ids: Vec<String> = answer.json()["items"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

// --------------------------------------------------------- the truth table --

#[tokio::test]
async fn the_instance_scope_sees_every_family_and_every_row() {
    let host = instance().await;

    let providers = host
        .send(keyed(get("/admin/api/providers"), "k-root"))
        .await;
    assert_eq!(providers.status, StatusCode::OK, "{}", providers.text());
    assert_eq!(ids(&providers), ["p1"]);

    let users = host.send(keyed(get("/admin/api/users"), "k-root")).await;
    assert_eq!(users.status, StatusCode::OK);

    let credentials = host
        .send(keyed(get("/admin/api/credentials"), "k-root"))
        .await;
    assert_eq!(
        ids(&credentials),
        ["c-acme", "c-core", "c-globex", "c-plain", "c-shared"]
    );

    let quotas = host.send(keyed(get("/admin/api/quotas"), "k-root")).await;
    assert_eq!(
        ids(&quotas),
        ["q-acme", "q-core", "q-globex", "q-limit", "q-user"]
    );
}

#[tokio::test]
async fn an_organization_scope_sees_two_families_and_only_its_own_rows() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;

    // Instance machinery and identity: refused, and refused before any row is
    // read.
    for closed in [
        "/admin/api/providers",
        "/admin/api/models",
        "/admin/api/routes",
        "/admin/api/settings",
        "/admin/api/users",
        "/admin/api/api-keys",
        "/admin/api/organizations",
        "/admin/api/teams",
        "/admin/api/permissions",
        "/admin/api/audit",
        "/admin/api/usage",
        "/admin/api/usage/records",
        "/admin/api/logs/downstream",
        "/admin/api/logs/upstream",
        "/admin/api/logs/captures/unknown",
        "/admin/api/logs/downstream/unknown",
        "/admin/api/channels",
        "/admin/api/tokenizer-vocabs",
    ] {
        let answer = as_session(&host, &session, None, closed).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{closed}");
        assert_eq!(answer.json()["error"]["code"], "forbidden", "{closed}");
    }

    // Credentials: the organization's own and its teams' by default, and one
    // owner's when it is named. Never the shared one, never another
    // organization's, and never a member's personal credential.
    let credentials = as_session(&host, &session, None, "/admin/api/credentials").await;
    assert_eq!(credentials.status, StatusCode::OK, "{}", credentials.text());
    assert_eq!(ids(&credentials), ["c-acme", "c-core"]);

    let team = as_session(
        &host,
        &session,
        None,
        "/admin/api/credentials?ownerKind=team&ownerId=core",
    )
    .await;
    assert_eq!(ids(&team), ["c-core"]);

    // Naming a foreign owner is an empty page, not an error: the filter is a
    // well-formed question whose answer is nothing.
    let foreign = as_session(
        &host,
        &session,
        None,
        "/admin/api/credentials?ownerKind=org&ownerId=globex",
    )
    .await;
    assert_eq!(foreign.status, StatusCode::OK);
    assert!(ids(&foreign).is_empty());
    assert_eq!(foreign.json()["total"], 0);

    // Quotas: the same narrowing, and a limit on a credential outside the
    // organization is not inside it either.
    let quotas = as_session(&host, &session, None, "/admin/api/quotas").await;
    assert_eq!(ids(&quotas), ["q-acme", "q-core"]);
    let team = as_session(
        &host,
        &session,
        None,
        "/admin/api/quotas?ownerKind=team&ownerId=core",
    )
    .await;
    assert_eq!(ids(&team), ["q-core"]);
    let limits = as_session(
        &host,
        &session,
        None,
        "/admin/api/quotas?ownerKind=credential&ownerId=c-shared",
    )
    .await;
    assert!(ids(&limits).is_empty());
}

#[tokio::test]
async fn a_team_scope_sees_only_the_teams_rows() {
    let host = instance().await;
    let session = cookie(&host, "lead").await;

    let credentials = as_session(&host, &session, None, "/admin/api/credentials").await;
    assert_eq!(credentials.status, StatusCode::OK, "{}", credentials.text());
    assert_eq!(ids(&credentials), ["c-core"]);

    let quotas = as_session(&host, &session, None, "/admin/api/quotas").await;
    assert_eq!(ids(&quotas), ["q-core"]);

    // The team's own parent organization is not the team's.
    let parent = as_session(
        &host,
        &session,
        None,
        "/admin/api/credentials?ownerKind=org&ownerId=acme",
    )
    .await;
    assert!(ids(&parent).is_empty());
    let row = as_session(&host, &session, None, "/admin/api/credentials/c-acme").await;
    assert_eq!(row.status, StatusCode::NOT_FOUND);

    assert_eq!(
        as_session(&host, &session, None, "/admin/api/providers")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_caller_who_administers_nothing_reaches_no_route_at_all() {
    let host = instance().await;
    let session = cookie(&host, "plain").await;

    for uri in [
        "/admin/api/credentials",
        "/admin/api/quotas",
        "/admin/api/providers",
        "/admin/api/users",
    ] {
        let answer = as_session(&host, &session, None, uri).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{uri}");
    }
    // A key bound to nothing is the same answer, which is what the surface
    // did before scopes existed.
    let answer = host
        .send(keyed(get("/admin/api/credentials"), "k-loose"))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);

    // Even the context route needs the caller to administer *something*.
    let answer = as_session(&host, &session, None, "/admin/api/context").await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
}

// ----------------------------------------------- rows, writes and headers --

#[tokio::test]
async fn a_row_outside_the_scope_is_not_found_and_never_forbidden() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;

    // The organization's own row is served…
    let mine = as_session(&host, &session, None, "/admin/api/credentials/c-acme").await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.text());
    // …a team's row inside it too…
    let team = as_session(&host, &session, None, "/admin/api/credentials/c-core").await;
    assert_eq!(team.status, StatusCode::OK);

    // …and everything else is indistinguishable from an id that never existed.
    for absent in [
        "/admin/api/credentials/c-globex",
        "/admin/api/credentials/c-shared",
        "/admin/api/credentials/c-plain",
        "/admin/api/credentials/never-minted",
        "/admin/api/quotas/q-globex",
        "/admin/api/quotas/q-user",
        "/admin/api/quotas/q-limit",
        "/admin/api/quotas/never-minted",
    ] {
        let answer = as_session(&host, &session, None, absent).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{absent}");
        assert_eq!(answer.json()["error"]["code"], "not_found", "{absent}");
    }

    // The same holds for the single-row actions, not only the reads.
    let reveal = host
        .send(with(
            with(
                post("/admin/api/credentials/c-globex/reveal", json!({})),
                "cookie",
                &session,
            ),
            "origin",
            "https://console.example.com",
        ))
        .await;
    assert_eq!(reveal.status, StatusCode::NOT_FOUND, "{}", reveal.text());

    // An operator limit reset names a row no organization owns.
    let limit = host
        .send(with(
            with(
                post("/admin/api/quotas/q-limit/limit-reset", json!({})),
                "cookie",
                &session,
            ),
            "origin",
            "https://console.example.com",
        ))
        .await;
    assert_eq!(limit.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_write_naming_an_owner_outside_the_scope_is_refused() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;

    let create = |body: Value| {
        with(
            with(post("/admin/api/quotas", body), "cookie", &session),
            "origin",
            "https://console.example.com",
        )
    };
    let budget = |owner_kind: &str, owner_id: &str| {
        json!({
            "ownerKind": owner_kind,
            "ownerId": owner_id,
            "metric": "cost",
            "unit": "USD",
            "limitValue": "10",
            "period": "total",
        })
    };

    // Its own organization and its own team: allowed.
    let mine = host.send(create(budget("org", "acme"))).await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.text());
    let team = host.send(create(budget("team", "core"))).await;
    assert_eq!(team.status, StatusCode::OK, "{}", team.text());

    // Another organization, another organization's team, a member's own
    // budget, and a provider limit: all refused, and refused as `Forbidden`
    // rather than `NotFound` — the caller typed the id, so there is nothing to
    // leak about whether it exists.
    for (kind, id) in [
        ("org", "globex"),
        ("team", "ops"),
        ("user", "plain"),
        ("provider", "p1"),
    ] {
        let answer = host.send(create(budget(kind, id))).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{kind}:{id}");
        assert_eq!(answer.json()["error"]["code"], "forbidden", "{kind}:{id}");
    }
    // A limit on a credential outside the scope names a credential the caller
    // cannot see, so it answers like every other read of one.
    let answer = host.send(create(budget("credential", "c-shared"))).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());

    // A patch that would move a row out of the scope is the same refusal, and
    // the row is unchanged.
    let mut patch = post(
        "/admin/api/quotas/q-acme",
        json!({ "ownerKind": "org", "ownerId": "globex" }),
    );
    *patch.method_mut() = Method::PATCH;
    let moved = host
        .send(with(
            with(patch, "cookie", &session),
            "origin",
            "https://console.example.com",
        ))
        .await;
    assert_eq!(moved.status, StatusCode::FORBIDDEN, "{}", moved.text());
    let still = as_session(&host, &session, None, "/admin/api/quotas/q-acme").await;
    assert_eq!(still.json()["ownerId"], "acme");

    // A credential create naming another organization is refused the same way.
    let credential = host
        .send(with(
            with(
                post(
                    "/admin/api/credentials",
                    json!({
                        "providerId": "p1",
                        "authKind": "api_key",
                        "secret": { "api_key": "x" },
                        "organizationId": "globex",
                    }),
                ),
                "cookie",
                &session,
            ),
            "origin",
            "https://console.example.com",
        ))
        .await;
    assert_eq!(credential.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_scope_header_naming_something_the_caller_does_not_administer_is_refused() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;

    for asked in ["organization:globex", "team:ops", "instance", "org:nobody"] {
        let answer = as_session(&host, &session, Some(asked), "/admin/api/credentials").await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{asked}");
    }
    // Its own, spelled either way, is accepted.
    for asked in ["organization:acme", "org:acme"] {
        let answer = as_session(&host, &session, Some(asked), "/admin/api/credentials").await;
        assert_eq!(answer.status, StatusCode::OK, "{asked}");
        assert_eq!(ids(&answer), ["c-acme", "c-core"]);
    }
    // A malformed header is a 400 rather than a silent fallback.
    let answer = as_session(&host, &session, Some("acme"), "/admin/api/credentials").await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_caller_with_several_scopes_must_name_one_but_still_reaches_context() {
    let host = instance().await;
    let session = cookie(&host, "both").await;

    let unnamed = as_session(&host, &session, None, "/admin/api/credentials").await;
    assert_eq!(
        unnamed.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unnamed.text()
    );
    assert!(unnamed.text().contains(SCOPE), "{}", unnamed.text());

    // The context route answers anyway, and is what the console offers the
    // choice from.
    let context = as_session(&host, &session, None, "/admin/api/context").await;
    assert_eq!(context.status, StatusCode::OK, "{}", context.text());
    let context = context.json();
    assert!(context["scope"].is_null());
    assert!(context["sections"].as_array().unwrap().is_empty());
    let selectors: Vec<&str> = context["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope["selector"].as_str().unwrap())
        .collect();
    assert_eq!(
        selectors,
        ["organization:acme", "organization:globex"],
        "{context}"
    );

    // Naming one settles it.
    let named = as_session(
        &host,
        &session,
        Some("organization:globex"),
        "/admin/api/credentials",
    )
    .await;
    assert_eq!(ids(&named), ["c-globex"]);
}

#[tokio::test]
async fn an_api_keys_binding_wins_over_any_header_it_sends() {
    let host = instance().await;

    // An organization-bound key, told to be the instance and to be another
    // organization. Neither changes anything.
    for asked in ["instance", "organization:globex", "team:core"] {
        let answer = host
            .send(with(
                keyed(get("/admin/api/credentials"), "k-acme-admin"),
                SCOPE,
                asked,
            ))
            .await;
        assert_eq!(answer.status, StatusCode::OK, "{asked}");
        assert_eq!(ids(&answer), ["c-acme", "c-core"], "{asked}");
    }

    // A team-bound key is the team, whatever it claims.
    let answer = host
        .send(with(
            keyed(get("/admin/api/credentials"), "k-core-lead"),
            SCOPE,
            "organization:acme",
        ))
        .await;
    assert_eq!(ids(&answer), ["c-core"]);

    // A binding picks the scope but does not grant it: any member may mint a
    // key bound to their organization, and that key administers nothing.
    for key in ["k-acme", "k-core"] {
        for uri in [
            "/admin/api/credentials",
            "/admin/api/quotas",
            "/admin/api/context",
        ] {
            let answer = host.send(keyed(get(uri), key)).await;
            assert_eq!(answer.status, StatusCode::FORBIDDEN, "{key} {uri}");
        }
    }

    // And the instance administrator's key ignores a header that would narrow
    // it, because the instance role is not a membership to be selected from.
    let answer = host
        .send(with(
            keyed(get("/admin/api/providers"), "k-root"),
            SCOPE,
            "organization:acme",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn a_credentials_limits_belong_to_whoever_owns_the_credential() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;
    let console = |request| {
        with(
            with(request, "cookie", &session),
            "origin",
            "https://console.example.com",
        )
    };
    let limit = |credential: &str| {
        json!({
            "ownerKind": "credential",
            "ownerId": credential,
            "metric": "requests",
            "unit": "count",
            "limitValue": "100",
            "period": "total",
        })
    };

    // A team credential inside the organization: create, list, read, delete.
    let created = host
        .send(console(post("/admin/api/quotas", limit("c-core"))))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_owned();
    let listed = as_session(
        &host,
        &session,
        None,
        "/admin/api/quotas?ownerKind=credential&ownerId=c-core",
    )
    .await;
    assert_eq!(ids(&listed), [id.as_str()]);
    let read = as_session(&host, &session, None, &format!("/admin/api/quotas/{id}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());

    // Moving it onto a credential outside the scope is refused.
    let mut moved = post(
        &format!("/admin/api/quotas/{id}"),
        json!({ "ownerId": "c-globex" }),
    );
    *moved.method_mut() = Method::PATCH;
    assert_eq!(
        host.send(console(moved)).await.status,
        StatusCode::NOT_FOUND
    );

    // The default list is budgets, not every limit of every credential.
    let quotas = as_session(&host, &session, None, "/admin/api/quotas").await;
    assert_eq!(ids(&quotas), ["q-acme", "q-core"]);

    // Another organization's credential, a member's personal one and the
    // shared one are not the organization's to cap.
    for foreign in ["c-globex", "c-plain", "c-shared"] {
        let answer = host
            .send(console(post("/admin/api/quotas", limit(foreign))))
            .await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{foreign}");
    }

    let mut delete = get(&format!("/admin/api/quotas/{id}"));
    *delete.method_mut() = Method::DELETE;
    let deleted = host.send(console(delete)).await;
    assert!(deleted.status.is_success(), "{}", deleted.text());

    // A team administrator does not reach its parent organization's
    // credential.
    let lead = cookie(&host, "lead").await;
    let answer = host
        .send(with(
            with(post("/admin/api/quotas", limit("c-acme")), "cookie", &lead),
            "origin",
            "https://console.example.com",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn how_a_credential_reaches_its_upstream_is_the_operators() {
    let host = instance().await;
    let session = cookie(&host, "orgadmin").await;
    let console = |request| {
        with(
            with(request, "cookie", &session),
            "origin",
            "https://console.example.com",
        )
    };
    let credential = |extra: Value| {
        let mut body = json!({
            "providerId": "p1",
            "authKind": "api_key",
            "secret": { "api_key": "x" },
            "organizationId": "acme",
        });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        body
    };
    let patch = |body: Value| {
        let mut request = post("/admin/api/credentials/c-acme", body);
        *request.method_mut() = Method::PATCH;
        request
    };

    for extra in [
        json!({ "proxy": { "mode": "explicit", "url": "http://169.254.169.254" } }),
        json!({ "connectionProfileId": "anything" }),
        // An address a channel reads out of the credential itself.
        json!({ "secret": { "api_key": "x", "base_url": "http://169.254.169.254/" } }),
        json!({ "metadata": { "oauth": { "token_endpoint": "http://127.0.0.1:1/token" } } }),
    ] {
        let created = host
            .send(console(post(
                "/admin/api/credentials",
                credential(extra.clone()),
            )))
            .await;
        assert_eq!(created.status, StatusCode::FORBIDDEN, "{extra}");
        let patched = host.send(console(patch(extra.clone()))).await;
        assert_eq!(patched.status, StatusCode::FORBIDDEN, "{extra}");
    }

    // Without them the same writes land.
    let created = host
        .send(console(post(
            "/admin/api/credentials",
            credential(json!({})),
        )))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let patched = host
        .send(console(patch(json!({ "label": "renamed" }))))
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
}

// ---------------------------------------------------------------- context --

#[tokio::test]
async fn the_context_route_is_what_a_console_renders_its_navigation_from() {
    let host = instance().await;

    let operator = host.send(keyed(get("/admin/api/context"), "k-root")).await;
    assert_eq!(operator.status, StatusCode::OK, "{}", operator.text());
    let operator = operator.json();
    assert_eq!(operator["user"]["id"], "root");
    assert_eq!(operator["user"]["instanceAdmin"], true);
    assert_eq!(operator["callerKind"], "apiKey");
    assert_eq!(operator["scopeHeader"], SCOPE);
    assert_eq!(operator["scope"]["kind"], "instance");
    let operator_sections = section_ids(&operator);
    assert!(operator_sections.contains(&"providers".to_owned()));
    assert!(operator_sections.contains(&"users".to_owned()));
    assert!(operator_sections.contains(&"credentials".to_owned()));

    let session = cookie(&host, "orgadmin").await;
    let scoped = as_session(&host, &session, None, "/admin/api/context").await;
    assert_eq!(scoped.status, StatusCode::OK, "{}", scoped.text());
    let scoped = scoped.json();
    assert_eq!(scoped["callerKind"], "session");
    assert_eq!(scoped["user"]["instanceAdmin"], false);
    assert_eq!(scoped["scope"]["kind"], "organization");
    assert_eq!(scoped["scope"]["id"], "acme");
    assert_eq!(scoped["scope"]["name"], "acme");
    assert_eq!(scoped["scope"]["selector"], "organization:acme");
    assert_eq!(scoped["scope"]["current"], true);
    // The whole navigation, and nothing else.
    assert_eq!(
        section_ids(&scoped),
        ["context", "session", "credentials", "quotas"]
    );

    // A team scope reports its parent organization, which is what a console
    // renders as the breadcrumb above it.
    let lead = cookie(&host, "lead").await;
    let lead = as_session(&host, &lead, None, "/admin/api/context").await;
    let lead = lead.json();
    assert_eq!(lead["scope"]["kind"], "team");
    assert_eq!(lead["scope"]["organizationId"], "acme");
    assert_eq!(section_ids(&lead).len(), 4);

    // Every section the context advertises is one the surface actually
    // serves: the navigation cannot offer a page that answers 403.
    for section in scoped["sections"].as_array().unwrap() {
        let path = section["path"].as_str().unwrap();
        if path.contains('{') {
            continue;
        }
        let answer = as_session(&host, &session, None, &format!("/admin/api{path}")).await;
        assert_eq!(answer.status, StatusCode::OK, "{path}: {}", answer.text());
    }
}

fn section_ids(context: &Value) -> Vec<String> {
    context["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn signing_out_does_not_need_a_settled_scope() {
    let host = instance().await;
    let session = cookie(&host, "both").await;

    let out = host
        .send(with(
            with(
                with(
                    request(Method::DELETE, "/admin/api/session"),
                    "cookie",
                    &session,
                ),
                "origin",
                "https://console.example.com",
            ),
            "host",
            support::HOST,
        ))
        .await;
    assert_eq!(out.status, StatusCode::OK, "{}", out.text());
    assert!(out.header("set-cookie").unwrap().contains("Max-Age=0"));
}
