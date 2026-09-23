#![cfg(not(target_arch = "wasm32"))]
//! The portal surface against a real database.
//!
//! Every test runs two users in two different organizations over one in-memory
//! instance, because the property under test is almost always the same one:
//! **nothing Alice asks can answer with Bob's data, and nothing Alice sends
//! can make it try.**
//!
//! Two shapes of that property appear here, and they are deliberately
//! different:
//!
//! - where an operation takes an id — a key, a grant — the row is read and its
//!   owner checked, and the answer for somebody else's id is `NotFound` (404),
//!   never `Forbidden`. A 403 would confirm the id exists;
//! - where an operation takes a filter — usage — the caller's id is *written
//!   into* the filter rather than compared against it, so the tests send
//!   another user's id and assert it was ignored.

mod support;

use gproxy_app::{
    App, Operations,
    dto::{PortalKeyCreate, PortalPasswordChange, PortalUsageQuery, UserPatch, UserWrite},
};
use gproxy_sdk::dto::{
    ExposedModelWrite, InstanceSettingsPatch, RouteMemberWrite, RouteWrite, SettingsPatch,
    UsageGroupBy,
};
use gproxy_store::entity::{
    identity::{api_key, membership_role::MembershipRole},
    oauth,
    usage::capture_record,
};
use sea_orm::{DatabaseConnection, Set};
use serde_json::{Value, json};

// ------------------------------------------------------------- fixture ----

/// Two users, two organizations, one shared team, two providers and a
/// permission rule that reaches only one of them for Alice.
///
/// Alice holds key `ka`, Bob holds key `kb`; a seeded key's plaintext is its
/// own id, so `support::caller_for(&app, "ka")` is Alice through the real
/// authenticator rather than a hand-built `Caller`.
async fn fixture() -> App<DatabaseConnection> {
    let (app, _client) = support::app().await;
    let handle = app.gproxy();

    support::person(handle, "alice", "user").await;
    support::person(handle, "bob", "user").await;
    support::organization(handle, "acme").await;
    support::organization(handle, "globex").await;
    support::team(handle, "core", "acme").await;
    support::org_member(handle, "acme", "alice", MembershipRole::Admin).await;
    support::team_member(handle, "core", "alice", MembershipRole::Member).await;
    support::org_member(handle, "globex", "bob", MembershipRole::Member).await;
    support::api_key(handle, "ka", "alice", None, None).await;
    support::api_key(handle, "kb", "bob", None, None).await;

    support::provider(handle, "p1", &["m1"]).await;
    support::provider(handle, "p2", &["m2"]).await;
    // Alice reaches p1 only; Bob reaches everything.
    support::allow(handle, "rule-alice", "alice", Some("p1")).await;
    support::allow(handle, "rule-bob", "bob", None).await;

    support::publish(&app).await;
    app
}

/// The status a failed operation answers with, which is the assertion almost
/// every scoping test makes.
fn status<T: std::fmt::Debug>(result: gproxy_app::Result<T>) -> u16 {
    result.unwrap_err().status_code()
}

// ------------------------------------------------------------- context ----

#[tokio::test]
async fn context_renders_the_callers_own_memberships_and_roles() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let context = ops.portal(&alice).context().await.unwrap();

    assert_eq!(context.user.id, "alice");
    assert_eq!(context.user.role, "user");
    assert!(!context.user.has_password);

    assert_eq!(context.organizations.len(), 1);
    assert_eq!(context.organizations[0].id, "acme");
    assert_eq!(context.organizations[0].role, "admin");

    assert_eq!(context.teams.len(), 1);
    assert_eq!(context.teams[0].id, "core");
    assert_eq!(context.teams[0].organization_id, "acme");
    assert_eq!(context.teams[0].role, "member");

    // Bob's organization is nowhere in Alice's context.
    let rendered = serde_json::to_string(&context).unwrap();
    assert!(!rendered.contains("globex"), "{rendered}");
    assert!(!rendered.contains("bob"), "{rendered}");

    // Flags come from configuration and the role, not from data.
    assert!(context.features.can_create_keys);
    assert!(context.features.can_change_password);
    assert!(context.features.can_see_logs, "the column defaults to on");
    assert!(
        !context.features.can_see_console,
        "a plain user is not offered the operator console"
    );
}

// -------------------------------------------------------------- models ----

#[tokio::test]
async fn models_reflect_the_callers_permissions() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());

    let models = ops.portal(&alice).models().unwrap();
    let named = |name: &str| {
        models
            .iter()
            .find(|model| model.name == name)
            .unwrap_or_else(|| panic!("`{name}` is missing from {models:?}"))
    };

    // Both channel forms are listed — nothing is omitted — and only the one
    // Alice's rule reaches is permitted.
    let permitted = named("test/m1");
    assert!(permitted.permitted);
    assert_eq!(permitted.provider_count, 1);
    assert_eq!(permitted.channel_ids, vec!["test".to_string()]);

    let denied = named("test/m2");
    assert!(
        !denied.permitted,
        "p2 is not in alice's rule, so its model is not callable"
    );
    // The name is still described: a count and a channel are configuration.
    assert_eq!(denied.provider_count, 1);

    // Bob's rule reaches every provider, so both are permitted for him.
    let bob = support::caller_for(&app, "kb").await;
    let his = ops.portal(&bob).models().unwrap();
    assert!(his.iter().all(|model| model.permitted), "{his:?}");

    // Sorted, so two calls agree.
    let names: Vec<&str> = models.iter().map(|model| model.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[tokio::test]
async fn an_exposed_name_is_listed_with_the_providers_behind_its_route() {
    let app = fixture().await;
    {
        let manage = app.gproxy().manage();
        manage
            .routes()
            .create(RouteWrite {
                id: Some("r1".into()),
                name: "r1".into(),
                ..RouteWrite::default()
            })
            .await
            .unwrap();
        for (member, provider) in [("m-p1", "p1"), ("m-p2", "p2")] {
            manage
                .route_members()
                .create(RouteMemberWrite {
                    id: Some(member.into()),
                    route_id: "r1".into(),
                    provider_id: provider.into(),
                    upstream_model: "m1".into(),
                    ..RouteMemberWrite::default()
                })
                .await
                .unwrap();
        }
        manage
            .exposed_models()
            .create(ExposedModelWrite {
                id: Some("e1".into()),
                name: "fast".into(),
                route_id: "r1".into(),
                ..ExposedModelWrite::default()
            })
            .await
            .unwrap();
    }
    app.reload_all().await.unwrap();

    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let models = ops.portal(&alice).models().unwrap();
    let exposed = models
        .iter()
        .find(|model| model.name == "fast")
        .expect("the exposed name is listed");
    assert_eq!(exposed.provider_count, 2, "both route members are live");
    assert_eq!(exposed.channel_ids, vec!["test".to_string()]);
    // One of the two members is a provider Alice may reach, so the name is.
    assert!(exposed.permitted);
}

// ---------------------------------------------------------------- keys ----

#[tokio::test]
async fn a_portal_user_mints_lists_rotates_reveals_and_deletes_their_own_keys() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    let created = portal
        .keys()
        .create(PortalKeyCreate {
            name: "laptop".into(),
            retain_secret: Some(true),
            ..PortalKeyCreate::default()
        })
        .await
        .unwrap();
    assert!(created.token.starts_with("sk-"));
    assert!(created.key.has_secret);
    let id = created.key.id.clone();

    // The seeded key and the minted one, and nothing of Bob's.
    let listed = portal.keys().list().await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|key| key.id.as_str()).collect();
    assert!(ids.contains(&"ka"));
    assert!(ids.contains(&id.as_str()));
    assert!(!ids.contains(&"kb"));

    // The plaintext is only ever in a mint, a rotation or a reveal.
    let listed_json = serde_json::to_value(&listed).unwrap();
    assert!(!listed_json.to_string().contains(&created.token));
    assert_eq!(
        portal.keys().reveal(&id).await.unwrap().token,
        created.token
    );

    let rotated = portal.keys().rotate(&id).await.unwrap();
    assert_ne!(rotated.token, created.token);
    assert_eq!(
        portal.keys().reveal(&id).await.unwrap().token,
        rotated.token
    );

    portal.keys().delete(&id).await.unwrap();
    let ids: Vec<String> = portal
        .keys()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|key| key.id)
        .collect();
    assert_eq!(ids, vec!["ka".to_string()]);
}

#[tokio::test]
async fn every_key_operation_answers_not_found_for_another_users_key() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    // Bob's key exists. From Alice's side it must be indistinguishable from
    // an id that was never minted — 404 for both, never 403.
    for id in ["kb", "never-minted"] {
        assert_eq!(status(portal.keys().rotate(id).await), 404, "rotate {id}");
        assert_eq!(status(portal.keys().reveal(id).await), 404, "reveal {id}");
        assert_eq!(status(portal.keys().delete(id).await), 404, "delete {id}");
    }

    // And Bob's key is still there: nothing was half-applied.
    let bob = support::caller_for(&app, "kb").await;
    let his: Vec<String> = ops
        .portal(&bob)
        .keys()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|key| key.id)
        .collect();
    assert_eq!(his, vec!["kb".to_string()]);
}

#[tokio::test]
async fn a_grants_internal_key_is_invisible_to_the_portal() {
    let app = fixture().await;
    seed_grant(&app, "g1", "alice", "codex").await;
    support::publish(&app).await;

    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    let ids: Vec<String> = portal
        .keys()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|key| key.id)
        .collect();
    assert_eq!(
        ids,
        vec!["ka".to_string()],
        "the oauth key is not a key here"
    );
    // And it cannot be reached by id either, for the same reason and with the
    // same answer.
    assert_eq!(status(portal.keys().reveal("oauth-g1").await), 404);
    assert_eq!(status(portal.keys().delete("oauth-g1").await), 404);
}

#[tokio::test]
async fn a_portal_key_cannot_be_bound_to_a_scope_the_caller_is_not_in() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    // Bob's organization: real, and not hers.
    let refused = portal
        .keys()
        .create(PortalKeyCreate {
            name: "borrowed".into(),
            organization_id: Some("globex".into()),
            ..PortalKeyCreate::default()
        })
        .await;
    assert_eq!(status(refused), 400);

    // Her own is accepted, with the binding on the row.
    let mine = portal
        .keys()
        .create(PortalKeyCreate {
            name: "team".into(),
            organization_id: Some("acme".into()),
            team_id: Some("core".into()),
            ..PortalKeyCreate::default()
        })
        .await
        .unwrap();
    assert_eq!(mine.key.organization_id.as_deref(), Some("acme"));
    assert_eq!(mine.key.team_id.as_deref(), Some("core"));
}

// --------------------------------------------------------------- usage ----

#[tokio::test]
async fn usage_is_scoped_to_the_caller_even_when_the_query_names_somebody_else() {
    let app = fixture().await;
    support::usage_row(app.gproxy(), "r-alice-1", "alice", 10).await;
    support::usage_row(app.gproxy(), "r-alice-2", "alice", 20).await;
    support::usage_row(app.gproxy(), "r-bob-1", "bob", 99).await;

    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());

    let usage = ops
        .portal(&alice)
        .usage(PortalUsageQuery {
            // Deliberately somebody else's. It must be overwritten, not
            // validated: a filter that is written cannot be forgotten.
            user_id: Some("bob".into()),
            group_by: Some(UsageGroupBy::User),
            ..PortalUsageQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(usage.summary.requests, 2, "alice's two rows, not bob's one");
    assert_eq!(usage.groups.len(), 1);
    assert_eq!(usage.groups[0].key.as_deref(), Some("alice"));

    // Bob sees his own, by the same mechanism.
    let bob = support::caller_for(&app, "kb").await;
    let his = ops
        .portal(&bob)
        .usage(PortalUsageQuery {
            user_id: Some("alice".into()),
            ..PortalUsageQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(his.summary.requests, 1);
}

#[tokio::test]
async fn a_trend_needs_a_range_and_a_grouped_cut_is_optional() {
    let app = fixture().await;
    support::usage_row(app.gproxy(), "r1", "alice", 1).await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    let plain = portal.usage(PortalUsageQuery::default()).await.unwrap();
    assert!(plain.groups.is_empty());
    assert!(plain.trend.is_empty());

    assert_eq!(
        status(
            portal
                .usage(PortalUsageQuery {
                    bucket_ms: Some(1_000),
                    ..PortalUsageQuery::default()
                })
                .await
        ),
        400
    );

    let trend = portal
        .usage(PortalUsageQuery {
            from_ms: Some(0),
            to_ms: Some(4_000),
            bucket_ms: Some(1_000),
            ..PortalUsageQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(trend.trend.len(), 4);
}

// --------------------------------------------------------------- quota ----

#[tokio::test]
async fn quota_reports_the_callers_own_budget_chain_and_nobody_elses() {
    let app = fixture().await;
    support::spent_budget(app.gproxy(), "q-alice", "user", "alice").await;
    support::spent_budget(app.gproxy(), "q-key", "api_key", "ka").await;
    support::spent_budget(app.gproxy(), "q-bob", "user", "bob").await;
    support::publish(&app).await;

    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let windows = ops.portal(&alice).quota().await.unwrap();

    let owners: Vec<(String, String)> = windows
        .iter()
        .map(|window| (window.owner_kind.clone(), window.owner_id.clone()))
        .collect();
    assert!(owners.contains(&("user".to_string(), "alice".to_string())));
    assert!(owners.contains(&("api_key".to_string(), "ka".to_string())));
    assert!(
        !owners.iter().any(|(_, id)| id == "bob"),
        "bob's budget is not in alice's chain: {owners:?}"
    );

    // A zero limit is a switched-off subject, not a ratio.
    let window = &windows[0];
    assert_eq!(window.limit, "0");
    assert_eq!(window.used_percent, None);
}

// ----------------------------------------------------- recent requests ----

#[tokio::test]
async fn recent_requests_are_gated_scoped_and_reduced() {
    let app = fixture().await;
    seed_capture(&app, "cap-alice", "alice", Some("ka"), Some("p1")).await;
    seed_capture(&app, "cap-bob", "bob", Some("kb"), Some("p1")).await;

    let alice = support::caller_for(&app, "ka").await;

    // On by default: the caller's own row only.
    let rows = {
        let data = app.data();
        let ops = Operations::new(app.gproxy(), &data, app.config());
        ops.portal(&alice).recent_requests(20).await.unwrap()
    };
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.request_id, "cap-alice");
    assert_eq!(row.api_key_id.as_deref(), Some("ka"));
    assert_eq!(row.model.as_deref(), Some("test/m1"));
    // The provider is a display name, and the id never travels.
    assert_eq!(row.provider.as_deref(), Some("p1"));
    assert_eq!(row.duration_ms, Some(120));
    assert_eq!(row.state, "completed");

    // What the admin log view carries and this deliberately does not.
    let rendered: Value = serde_json::to_value(row).unwrap();
    let object = rendered.as_object().unwrap();
    for absent in [
        "requestBody",
        "responseBody",
        "requestHeaders",
        "responseHeaders",
        "requestUrl",
        "requestMethod",
        "clientIp",
        "credentialId",
        "providerId",
        "userId",
        "sessionId",
        "error",
    ] {
        assert!(!object.contains_key(absent), "{absent} leaked: {rendered}");
    }
    assert!(!rendered.to_string().contains("secret-prompt"));
    assert!(!rendered.to_string().contains("cred-1"));

    // Off: an empty list, not an error.
    app.gproxy()
        .manage()
        .settings()
        .update(SettingsPatch {
            instance: Some(InstanceSettingsPatch {
                portal_recent_requests_enabled: Some(false),
                ..InstanceSettingsPatch::default()
            }),
            ..SettingsPatch::default()
        })
        .await
        .unwrap();
    app.reload_all().await.unwrap();

    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);
    assert!(portal.recent_requests(20).await.unwrap().is_empty());
    // And the flag says so, so a console can hide the tab instead.
    assert!(!portal.context().await.unwrap().features.can_see_logs);
}

// ------------------------------------------------------ oauth sessions ----

#[tokio::test]
async fn oauth_sessions_list_and_revoke_are_scoped_to_the_caller() {
    let app = fixture().await;
    seed_grant(&app, "g-alice", "alice", "codex").await;
    seed_grant(&app, "g-bob", "bob", "codex").await;
    // Rows written straight through the store do not move the revision, and a
    // snapshot only advances when it does: bump it the way a peer's write
    // would, so the client registration reaches `AppData`.
    support::publish(&app).await;

    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    let sessions = portal.oauth_sessions().list().await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "g-alice");
    assert_eq!(sessions[0].client_id, "codex");
    assert_eq!(sessions[0].client_name.as_deref(), Some("Codex CLI"));
    assert_eq!(sessions[0].scopes, vec!["openid".to_string()]);

    // Bob's grant is a 404 from here, and a second revoke of her own is too:
    // it is no longer a live grant of hers.
    assert_eq!(status(portal.oauth_sessions().revoke("g-bob").await), 404);
    assert_eq!(status(portal.oauth_sessions().revoke("absent").await), 404);
    portal.oauth_sessions().revoke("g-alice").await.unwrap();
    assert_eq!(status(portal.oauth_sessions().revoke("g-alice").await), 404);
    assert!(portal.oauth_sessions().list().await.unwrap().is_empty());

    // Revocation disabled the grant's internal key, and left Bob's alone.
    let keys = app
        .gproxy()
        .store()
        .api_keys()
        .get_many(&["oauth-g-alice".to_string(), "oauth-g-bob".to_string()])
        .await
        .unwrap();
    assert!(!keys[0].as_ref().unwrap().enabled);
    assert!(keys[1].as_ref().unwrap().enabled);

    let bob = support::caller_for(&app, "kb").await;
    assert_eq!(
        ops.portal(&bob)
            .oauth_sessions()
            .list()
            .await
            .unwrap()
            .len(),
        1
    );
}

// ------------------------------------------------------------ password ----

#[tokio::test]
async fn a_password_change_proves_the_old_one_and_ends_every_session() {
    let app = fixture().await;
    {
        let data = app.data();
        let ops = Operations::new(app.gproxy(), &data, app.config());
        ops.users()
            .set_password("alice", "correct horse battery")
            .await
            .unwrap();
    }
    app.reload_all().await.unwrap();

    // Two live sessions, as if she were signed in on two machines.
    let data = app.data();
    let first = app
        .authenticator(&data)
        .create_session("alice", 1_000)
        .await
        .unwrap();
    let second = app
        .authenticator(&data)
        .create_session("alice", 1_000)
        .await
        .unwrap();

    let alice = support::caller_for(&app, "ka").await;
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    // A wrong current password is a 403 and changes nothing: the session that
    // asked is still valid, so 401 would be the wrong answer.
    let refused = portal
        .password()
        .change(PortalPasswordChange {
            current: Some("wrong".into()),
            new: "a new long password".into(),
        })
        .await;
    assert_eq!(status(refused), 403);
    assert!(
        app.authenticator(&data)
            .authenticate_session(&first.token, 2_000)
            .await
            .is_ok()
    );

    // An account that has a password may not omit it.
    assert_eq!(
        status(
            portal
                .password()
                .change(PortalPasswordChange {
                    current: None,
                    new: "a new long password".into(),
                })
                .await
        ),
        400
    );

    portal
        .password()
        .change(PortalPasswordChange {
            current: Some("correct horse battery".into()),
            new: "a new long password".into(),
        })
        .await
        .unwrap();

    for token in [&first.token, &second.token] {
        assert!(
            app.authenticator(&data)
                .authenticate_session(token, 2_000)
                .await
                .is_err(),
            "a password change signs the account out everywhere"
        );
    }
    // The new password is the one that works now.
    assert!(
        ops.portal_login("alice", "a new long password")
            .await
            .is_ok()
    );
    assert!(
        ops.portal_login("alice", "correct horse battery")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn an_oauth_only_account_sets_a_password_without_proving_one() {
    let app = fixture().await;
    let alice = support::caller_for(&app, "ka").await;
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());
    let portal = ops.portal(&alice);

    // Alice has no password yet. Sending one is refused rather than ignored.
    assert_eq!(
        status(
            portal
                .password()
                .change(PortalPasswordChange {
                    current: Some("anything".into()),
                    new: "a first password".into(),
                })
                .await
        ),
        400
    );

    portal
        .password()
        .change(PortalPasswordChange {
            current: None,
            new: "a first password".into(),
        })
        .await
        .unwrap();
    assert!(ops.portal_login("alice", "a first password").await.is_ok());
}

// -------------------------------------------------------- login/logout ----

#[tokio::test]
async fn a_sign_in_opens_a_session_and_a_sign_out_ends_it() {
    let app = fixture().await;
    {
        let data = app.data();
        let ops = Operations::new(app.gproxy(), &data, app.config());
        ops.users()
            .create(UserWrite {
                id: Some("carol".into()),
                name: "carol".into(),
                password: Some("carol's long password".into()),
                ..UserWrite::default()
            })
            .await
            .unwrap();
    }
    app.reload_all().await.unwrap();
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());

    let session = ops
        .portal_login_at("carol", "carol's long password", 1_000)
        .await
        .unwrap();
    let caller = app
        .authenticator(&data)
        .authenticate_session(&session.token, 2_000)
        .await
        .unwrap();
    assert_eq!(caller.user_id, "carol");
    assert_eq!(caller.api_key_id, None, "a session caller is not a key");

    // Logging out through the portal ends exactly that session.
    assert!(ops.portal(&caller).logout(&session.token).await.unwrap());
    assert!(!ops.portal(&caller).logout(&session.token).await.unwrap());
    assert!(
        app.authenticator(&data)
            .authenticate_session(&session.token, 2_000)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn every_way_a_sign_in_can_fail_answers_the_same_unauthorized() {
    let app = fixture().await;
    {
        let data = app.data();
        let ops = Operations::new(app.gproxy(), &data, app.config());
        ops.users()
            .set_password("alice", "correct horse battery")
            .await
            .unwrap();
        ops.users()
            .update(
                "bob",
                UserPatch {
                    enabled: Some(false),
                    ..UserPatch::default()
                },
            )
            .await
            .unwrap();
    }
    app.reload_all().await.unwrap();
    let data = app.data();
    let ops = Operations::new(app.gproxy(), &data, app.config());

    for (name, password) in [
        ("alice", "wrong"),
        ("nobody", "correct horse battery"),
        // Bob is disabled, and has no password anyway.
        ("bob", "correct horse battery"),
        ("", ""),
        ("alice", ""),
    ] {
        assert_eq!(
            status(ops.portal_login(name, password).await),
            401,
            "{name}/{password}"
        );
    }
    assert!(
        ops.portal_login("  alice  ", "correct horse battery")
            .await
            .is_ok()
    );
}

// ------------------------------------------------------------- seeding ----

/// A registered client, a grant, and the grant's internal `oauth` key.
async fn seed_grant(app: &App<DatabaseConnection>, grant_id: &str, user_id: &str, client: &str) {
    let store = app.gproxy().store();
    // Idempotent: two grants in one test share one client registration.
    if store
        .oauth_clients()
        .get_many(&[client.to_string()])
        .await
        .unwrap()
        .into_iter()
        .next()
        .flatten()
        .is_none()
    {
        store
            .oauth_clients()
            .create_many(vec![oauth::client::ActiveModel {
                id: Set(client.into()),
                name: Set("Codex CLI".into()),
                redirect_uris: Set(json!(["http://127.0.0.1/callback"])),
                enabled: Set(true),
                ..Default::default()
            }])
            .await
            .unwrap();
    }
    let key_id = format!("oauth-{grant_id}");
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set(key_id.clone()),
            user_id: Set(user_id.into()),
            name: Set(format!("{client} grant")),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set(gproxy_app::snapshot::encode_key_hash(&support::digest_of(
                &key_id,
            ))),
            prefix: Set("at-".into()),
            enabled: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_grants()
        .create_many(vec![oauth::grant::ActiveModel {
            id: Set(grant_id.into()),
            user_id: Set(user_id.into()),
            api_key_id: Set(key_id),
            client_id: Set(client.into()),
            scopes: Set(json!(["openid"])),
            subject: Set(user_id.into()),
            created_at_ms: Set(1_000),
            logged_in_at_ms: Set(Some(1_100)),
            last_refreshed_at_ms: Set(Some(1_200)),
            refresh_count: Set(3),
            refresh_expires_at_ms: Set(Some(9_000)),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// One downstream capture row carrying everything the portal must not report.
async fn seed_capture(
    app: &App<DatabaseConnection>,
    id: &str,
    user_id: &str,
    api_key_id: Option<&str>,
    provider_id: Option<&str>,
) {
    app.gproxy()
        .store()
        .capture_records()
        .create_many(vec![capture_record::ActiveModel {
            id: Set(id.into()),
            side: Set(capture_record::CaptureSide::Downstream),
            kind: Set(capture_record::CaptureKind::Http),
            user_id: Set(Some(user_id.into())),
            api_key_id: Set(api_key_id.map(Into::into)),
            provider_id: Set(provider_id.map(Into::into)),
            credential_id: Set(Some("cred-1".into())),
            model: Set(Some("test/m1".into())),
            operation: Set(Some("generate_content".into())),
            request_method: Set(Some("POST".into())),
            request_url: Set(Some("/v1/responses".into())),
            request_headers: Set(Some(json!([["authorization", "Bearer secret-prompt"]]))),
            request_body: Set(Some(b"{\"prompt\":\"secret-prompt\"}".to_vec())),
            response_status: Set(Some(200)),
            client_ip: Set(Some("203.0.113.7".into())),
            state: Set(capture_record::CaptureState::Completed),
            error: Set(Some("secret-prompt".into())),
            started_at_ms: Set(1_000),
            ended_at_ms: Set(Some(1_120)),
            ..Default::default()
        }])
        .await
        .unwrap();
}
