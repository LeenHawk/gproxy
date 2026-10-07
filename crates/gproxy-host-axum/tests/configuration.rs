#![cfg(not(target_arch = "wasm32"))]
//! The configuration half of `/admin/api`, through the router.
//!
//! What is tested here is the **binding**, not the operations: the sdk's own
//! suite already asserts that a write lands one revision and that a rule
//! compiles. What only this crate can get wrong is the route table — a path
//! that does not exist, a method that was never declared, a handler wired to
//! the wrong family, an answer rendered with the wrong status, a write that
//! escapes the audit trail.
//!
//! So every test here sends a real `http::Request` through the real router
//! over a real in-memory instance, and the last one walks the whole table.

mod support;

use gproxy_store::entity::{identity::audit_event, upstream::provider};
use http::{Method, StatusCode};
use sea_orm::EntityTrait;
use serde_json::{Value, json};
use support::{Answer, Host, get, keyed, post, request};

/// The administrator's key. `support::api_key` makes a key's plaintext its own
/// id, so this is both.
const KEY: &str = "k-root";

/// One instance administrator and one ordinary account.
async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, KEY, "root", None, None).await;
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    host.publish().await;
    host
}

/// A JSON request on the operator's surface, as the administrator.
async fn call(host: &Host, method: Method, path: &str, body: Value) -> Answer {
    let mut request = post(path, body);
    *request.method_mut() = method;
    host.send(keyed(request, KEY)).await
}

async fn read(host: &Host, path: &str) -> Answer {
    host.send(keyed(get(path), KEY)).await
}

/// Create a row through the family's collection route and hand back its id,
/// having first proved the answer is the row the caller asked for.
async fn create(host: &Host, path: &str, body: Value) -> String {
    let answer = call(host, Method::POST, path, body).await;
    assert_eq!(answer.status, StatusCode::OK, "{path}: {}", answer.text());
    let id = answer.json()["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{path} answered without an id: {}", answer.text()))
        .to_owned();
    // The row as the database now holds it, not the echo of the request.
    let row = read(host, &format!("{path}/{id}")).await;
    assert_eq!(row.status, StatusCode::OK, "{path}/{id}: {}", row.text());
    assert_eq!(row.json()["id"], id);
    id
}

async fn audit_rows(host: &Host) -> Vec<audit_event::Model> {
    host.app
        .gproxy()
        .store()
        .audit_events()
        .query(audit_event::Entity::find())
        .await
        .unwrap()
}

/// The durable counter every configuration write advances.
async fn revision(host: &Host) -> i64 {
    let answer = read(host, "/admin/api/settings").await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    answer.json()["instance"]["configRevision"]
        .as_i64()
        .unwrap_or_else(|| panic!("no revision in {}", answer.text()))
}

// ------------------------------------------------------------ the guard --

#[tokio::test]
async fn the_configuration_surface_needs_a_credential_and_an_instance_admin() {
    let host = instance().await;

    let answer = host.send(get("/admin/api/providers")).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(answer.json()["error"]["code"], "unauthorized");

    // A real, valid key that is not an instance administrator's.
    let answer = host
        .send(keyed(get("/admin/api/providers"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json()["error"]["code"], "forbidden");

    // And a write, so the refusal is not a property of `GET`.
    let answer = host
        .send(keyed(
            post(
                "/admin/api/providers",
                json!({ "name": "p1", "channel": "test" }),
            ),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert!(
        host.app
            .gproxy()
            .store()
            .providers()
            .query(provider::Entity::find())
            .await
            .unwrap()
            .is_empty(),
        "a refused write must not reach the database"
    );

    let answer = read(&host, "/admin/api/providers").await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(answer.json()["items"].is_array());
}

// --------------------------------------------------------- the families --

#[tokio::test]
async fn every_configuration_family_round_trips_through_the_router() {
    let host = instance().await;

    let provider = create(
        &host,
        "/admin/api/providers",
        json!({ "name": "p1", "channel": "test", "baseUrl": "https://p1.example" }),
    )
    .await;
    // The row the write actually left, read from the table rather than from
    // the answer: the binding could have echoed anything.
    let rows: Vec<provider::Model> = host
        .app
        .gproxy()
        .store()
        .providers()
        .query(provider::Entity::find())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "p1");
    assert_eq!(rows[0].channel, "test");
    assert_eq!(rows[0].base_url.as_deref(), Some("https://p1.example"));

    let credential = create(
        &host,
        "/admin/api/credentials",
        json!({
            "providerId": provider,
            "authKind": "api_key",
            "label": "first",
            "secret": { "api_key": "k-1" },
        }),
    )
    .await;
    let row = read(&host, &format!("/admin/api/credentials/{credential}")).await;
    assert_eq!(row.json()["hasSecret"], true);
    assert!(
        row.json().get("secret").is_none(),
        "a credential DTO never carries the sealed bytes: {}",
        row.text()
    );

    let model = create(&host, "/admin/api/models", json!({ "name": "m1" })).await;
    let provider_model = create(
        &host,
        "/admin/api/provider-models",
        json!({ "providerId": provider, "upstreamName": "m1-upstream" }),
    )
    .await;

    let route = create(&host, "/admin/api/routes", json!({ "name": "fast" })).await;
    let member = create(
        &host,
        "/admin/api/route-members",
        json!({
            "routeId": route,
            "providerId": provider,
            "upstreamModel": "m1-upstream",
        }),
    )
    .await;

    let profile = create(
        &host,
        "/admin/api/connection-profiles",
        json!({ "name": "direct", "backend": "reqwest" }),
    )
    .await;

    let rule_set = create(
        &host,
        "/admin/api/rule-sets",
        json!({ "name": "redactions" }),
    )
    .await;
    let rule = create(
        &host,
        "/admin/api/rules",
        json!({
            "ruleSetId": rule_set,
            "pattern": "secret",
            "replacement": "[redacted]",
        }),
    )
    .await;
    let binding = create(
        &host,
        "/admin/api/provider-rule-sets",
        json!({ "providerId": provider, "ruleSetId": rule_set }),
    )
    .await;

    let operation_rule = create(
        &host,
        "/admin/api/operation-rules",
        json!({
            "providerId": provider,
            "operation": "generate_content",
            "action": "deny",
        }),
    )
    .await;
    let endpoint = create(
        &host,
        "/admin/api/operation-endpoints",
        json!({
            "providerId": provider,
            "operation": "generate_content",
            "dialect": "openai",
            "url": "https://upstream.example/v1/responses",
        }),
    )
    .await;

    let quota = create(
        &host,
        "/admin/api/quotas",
        json!({
            "ownerKind": "user",
            "ownerId": "u1",
            "metric": "cost",
            "unit": "USD",
            "limitValue": "25",
            "period": "1d",
        }),
    )
    .await;

    let price_rule = create(
        &host,
        "/admin/api/price-rules",
        json!({ "modelPattern": "m1*", "currency": "usd" }),
    )
    .await;
    let price_rate = create(
        &host,
        "/admin/api/price-rates",
        json!({
            "priceRuleId": price_rule,
            "metric": "input_tokens",
            "unit": "token",
            "unitQuantity": "1000000",
            "value": "3",
        }),
    )
    .await;
    let price_tier = create(
        &host,
        "/admin/api/price-tiers",
        json!({
            "priceRuleId": price_rule,
            "minPromptTokens": 200_000,
            "inputPerMillion": "6",
        }),
    )
    .await;

    // Settings is the one family with a single row: no collection, no id.
    let patched = call(
        &host,
        Method::PATCH,
        "/admin/api/settings",
        json!({ "instance": { "instanceName": "acme" } }),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
    assert_eq!(patched.json()["instance"]["instanceName"], "acme");
    assert_eq!(
        read(&host, "/admin/api/settings").await.json()["instance"]["instanceName"],
        "acme"
    );

    // Every id the families minted is distinct, which is the cheapest check
    // that no two routes were wired to the same family.
    let ids = [
        &provider,
        &credential,
        &model,
        &provider_model,
        &route,
        &member,
        &profile,
        &rule_set,
        &rule,
        &binding,
        &operation_rule,
        &endpoint,
        &quota,
        &price_rule,
        &price_rate,
        &price_tier,
    ];
    let unique: std::collections::BTreeSet<&String> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "{ids:?}");

    // And a delete is a 204 rather than an empty object, then a 404.
    let deleted = call(
        &host,
        Method::DELETE,
        &format!("/admin/api/price-tiers/{price_tier}"),
        json!({}),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert!(deleted.bytes.is_empty());
    let again = call(
        &host,
        Method::DELETE,
        &format!("/admin/api/price-tiers/{price_tier}"),
        json!({}),
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert_eq!(again.json()["error"]["code"], "not_found");
}

#[tokio::test]
async fn a_configuration_write_bumps_the_revision_and_leaves_a_trail_row() {
    let host = instance().await;
    let before = revision(&host).await;

    let provider = create(
        &host,
        "/admin/api/providers",
        json!({ "name": "p1", "channel": "test" }),
    )
    .await;
    let after = revision(&host).await;
    assert_eq!(
        after,
        before + 1,
        "one write, one revision: {before} -> {after}"
    );

    let actions: Vec<String> = audit_rows(&host)
        .await
        .into_iter()
        .map(|row| row.action)
        .collect();
    assert!(
        actions.contains(&"admin.providers.create".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.iter().any(|action| action.ends_with(".list")),
        "non-channel reads must be audited: {actions:?}"
    );
    assert!(
        actions.iter().any(|action| action.ends_with(".get")),
        "non-channel reads must be audited: {actions:?}"
    );

    // A batch is one transaction however many rows it names, so it is one
    // revision too.
    let before = revision(&host).await;
    let batch = call(
        &host,
        Method::POST,
        "/admin/api/provider-models/batch",
        json!([
            { "create": { "providerId": provider, "upstreamName": "m1" } },
            { "create": { "providerId": provider, "upstreamName": "m2" } },
        ]),
    )
    .await;
    assert_eq!(batch.status, StatusCode::OK, "{}", batch.text());
    assert_eq!(batch.json().as_array().unwrap().len(), 2);
    assert_eq!(revision(&host).await, before + 1);
}

// ------------------------------------------------- the two careful routes --

#[tokio::test]
async fn revealing_a_secret_is_audited_without_recording_the_secret() {
    let host = instance().await;
    let provider = create(
        &host,
        "/admin/api/providers",
        json!({ "name": "p1", "channel": "test" }),
    )
    .await;
    let credential = create(
        &host,
        "/admin/api/credentials",
        json!({
            "providerId": provider,
            "authKind": "api_key",
            "secret": { "api_key": "k-1" },
        }),
    )
    .await;

    let revealed = call(
        &host,
        Method::POST,
        &format!("/admin/api/credentials/{credential}/reveal"),
        json!({}),
    )
    .await;
    assert_eq!(revealed.status, StatusCode::OK, "{}", revealed.text());
    assert_eq!(revealed.json(), json!({ "api_key": "k-1" }));

    let actions: Vec<String> = audit_rows(&host)
        .await
        .into_iter()
        .map(|row| row.action)
        .collect();
    assert!(
        actions.contains(&"admin.credentials.reveal".to_owned()),
        "a disclosure must be accountable afterwards: {actions:?}"
    );
}

#[tokio::test]
async fn an_export_is_never_cached() {
    let host = instance().await;
    create(
        &host,
        "/admin/api/providers",
        json!({ "name": "p1", "channel": "test" }),
    )
    .await;

    let exported = call(
        &host,
        Method::POST,
        "/admin/api/export",
        json!({ "includeSecrets": true }),
    )
    .await;
    assert_eq!(exported.status, StatusCode::OK, "{}", exported.text());
    assert_eq!(
        exported.header("cache-control"),
        Some("no-store"),
        "an export can carry sealed secrets and must not reach a disk cache"
    );
    assert_eq!(exported.json()["data"]["providers"][0]["name"], "p1");

    // And it is refused to an ordinary account like everything else here.
    let refused = host
        .send(keyed(
            post("/admin/api/export", json!({ "includeSecrets": true })),
            "k-alice",
        ))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

// -------------------------------------------------------- the catalogues --

#[tokio::test]
async fn the_catalogues_answer_from_this_binary() {
    let host = instance().await;

    let channels = read(&host, "/admin/api/channels").await;
    assert_eq!(channels.status, StatusCode::OK, "{}", channels.text());
    let listed = channels.json();
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|channel| channel["id"].as_str().unwrap())
        .collect();
    // A descriptor goes over the wire as itself, which is what a console
    // renders a provider form from.
    assert_eq!(ids, ["test"], "{}", channels.text());

    assert_eq!(
        read(&host, "/admin/api/tls-presets").await.status,
        StatusCode::OK
    );
    let presets = read(&host, "/admin/api/rule-presets").await;
    assert_eq!(presets.status, StatusCode::OK, "{}", presets.text());
    assert!(!presets.json().as_array().unwrap().is_empty());

    let catalog = read(&host, "/admin/api/default-model-catalog").await;
    assert_eq!(catalog.status, StatusCode::OK, "{}", catalog.text());
    assert!(catalog.json()["models"].as_array().unwrap().len() > 1);
}

#[tokio::test]
async fn a_rule_set_is_replaced_wholesale_and_a_preset_is_one_of_those_replacements() {
    let host = instance().await;
    let rule_set = create(
        &host,
        "/admin/api/rule-sets",
        json!({ "name": "redactions" }),
    )
    .await;

    let replaced = call(
        &host,
        Method::PUT,
        &format!("/admin/api/rule-sets/{rule_set}/rules"),
        json!([
            { "pattern": "secret", "replacement": "[redacted]" },
            { "pattern": "token", "replacement": "[redacted]" },
        ]),
    )
    .await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let rules = replaced.json();
    assert_eq!(rules.as_array().unwrap().len(), 2);
    assert_eq!(rules[0]["sortOrder"], 0);
    assert_eq!(rules[1]["sortOrder"], 1, "caller order is rule order");

    // A preset is a complete replacement of the same kind, and both ids come
    // from the path.
    let applied = call(
        &host,
        Method::POST,
        &format!("/admin/api/rule-sets/{rule_set}/rule-presets/cline"),
        json!({}),
    )
    .await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text());
    assert!(!applied.json().as_array().unwrap().is_empty());

    let missing = call(
        &host,
        Method::POST,
        &format!("/admin/api/rule-sets/{rule_set}/rule-presets/does-not-exist"),
        json!({}),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND, "{}", missing.text());
}

#[tokio::test]
async fn a_budget_status_is_asked_for_by_owner_and_a_malformed_owner_is_a_400() {
    let host = instance().await;
    create(
        &host,
        "/admin/api/quotas",
        json!({
            "ownerKind": "user",
            "ownerId": "u1",
            "metric": "cost",
            "unit": "USD",
            "limitValue": "25",
            "period": "1d",
        }),
    )
    .await;

    let status = read(&host, "/admin/api/quotas/status?owners=user:u1").await;
    assert_eq!(status.status, StatusCode::OK, "{}", status.text());
    let windows = status.json();
    assert_eq!(windows.as_array().unwrap().len(), 1, "{}", status.text());
    assert_eq!(windows[0]["limit"], "25");
    assert_eq!(windows[0]["used"], "0");

    // An owner that is not `kind:id` is the request's fault, not the
    // instance's.
    let bad = read(&host, "/admin/api/quotas/status?owners=nonsense").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.text());
    assert_eq!(bad.json()["error"]["code"], "invalid_request");
}

// ---------------------------------------------------------- the whole table --

/// Every configuration route, by the method it is declared with.
///
/// The `{id}` positions are filled with a value nothing matches on purpose:
/// what is being checked is the **route**, not the row.
const TABLE: &[(Method, &str)] = &[
    (Method::GET, "/admin/api/providers"),
    (Method::POST, "/admin/api/providers"),
    (Method::POST, "/admin/api/providers/batch"),
    (Method::GET, "/admin/api/providers/x"),
    (Method::PATCH, "/admin/api/providers/x"),
    (Method::DELETE, "/admin/api/providers/x"),
    (
        Method::POST,
        "/admin/api/providers/x/routing-defaults/reset",
    ),
    (Method::GET, "/admin/api/credentials"),
    (Method::POST, "/admin/api/credentials"),
    (Method::POST, "/admin/api/credentials/batch"),
    (Method::GET, "/admin/api/credentials/x"),
    (Method::PATCH, "/admin/api/credentials/x"),
    (Method::DELETE, "/admin/api/credentials/x"),
    (Method::POST, "/admin/api/credentials/x/reveal"),
    (Method::POST, "/admin/api/credentials/x/status"),
    (Method::POST, "/admin/api/credentials/x/refresh"),
    (Method::GET, "/admin/api/credentials/x/quota"),
    (Method::GET, "/admin/api/credentials/x/quota-observations"),
    (Method::POST, "/admin/api/credentials/x/quota-probe"),
    (Method::POST, "/admin/api/credentials/x/quota-diagnostics"),
    (Method::POST, "/admin/api/credentials/x/quota-reset"),
    (Method::POST, "/admin/api/credentials/x/health-reset"),
    (Method::GET, "/admin/api/credentials/x/limits"),
    (Method::GET, "/admin/api/models"),
    (Method::POST, "/admin/api/models"),
    (Method::POST, "/admin/api/models/batch"),
    (Method::GET, "/admin/api/models/x"),
    (Method::PATCH, "/admin/api/models/x"),
    (Method::DELETE, "/admin/api/models/x"),
    (Method::POST, "/admin/api/models/discover"),
    (Method::GET, "/admin/api/models/openrouter"),
    (Method::POST, "/admin/api/models/discover/apply"),
    (Method::POST, "/admin/api/models/test"),
    (Method::GET, "/admin/api/provider-models"),
    (Method::POST, "/admin/api/provider-models"),
    (Method::POST, "/admin/api/provider-models/batch"),
    (Method::GET, "/admin/api/provider-models/x"),
    (Method::PATCH, "/admin/api/provider-models/x"),
    (Method::DELETE, "/admin/api/provider-models/x"),
    (Method::GET, "/admin/api/routes"),
    (Method::POST, "/admin/api/routes"),
    (Method::POST, "/admin/api/routes/batch"),
    (Method::GET, "/admin/api/routes/x"),
    (Method::PATCH, "/admin/api/routes/x"),
    (Method::DELETE, "/admin/api/routes/x"),
    (Method::GET, "/admin/api/route-members"),
    (Method::POST, "/admin/api/route-members"),
    (Method::POST, "/admin/api/route-members/batch"),
    (Method::GET, "/admin/api/route-members/x"),
    (Method::PATCH, "/admin/api/route-members/x"),
    (Method::DELETE, "/admin/api/route-members/x"),
    (Method::GET, "/admin/api/connection-profiles"),
    (Method::POST, "/admin/api/connection-profiles"),
    (Method::POST, "/admin/api/connection-profiles/batch"),
    (Method::GET, "/admin/api/connection-profiles/x"),
    (Method::PATCH, "/admin/api/connection-profiles/x"),
    (Method::DELETE, "/admin/api/connection-profiles/x"),
    (Method::GET, "/admin/api/settings"),
    (Method::PATCH, "/admin/api/settings"),
    (Method::GET, "/admin/api/rule-sets"),
    (Method::POST, "/admin/api/rule-sets"),
    (Method::POST, "/admin/api/rule-sets/batch"),
    (Method::GET, "/admin/api/rule-sets/x"),
    (Method::PATCH, "/admin/api/rule-sets/x"),
    (Method::DELETE, "/admin/api/rule-sets/x"),
    (Method::PUT, "/admin/api/rule-sets/x/rules"),
    (Method::POST, "/admin/api/rule-sets/x/rule-presets/cline"),
    (Method::GET, "/admin/api/rules"),
    (Method::POST, "/admin/api/rules"),
    (Method::POST, "/admin/api/rules/batch"),
    (Method::GET, "/admin/api/rules/x"),
    (Method::PATCH, "/admin/api/rules/x"),
    (Method::DELETE, "/admin/api/rules/x"),
    (Method::GET, "/admin/api/provider-rule-sets"),
    (Method::POST, "/admin/api/provider-rule-sets"),
    (Method::POST, "/admin/api/provider-rule-sets/batch"),
    (Method::GET, "/admin/api/provider-rule-sets/x"),
    (Method::PATCH, "/admin/api/provider-rule-sets/x"),
    (Method::DELETE, "/admin/api/provider-rule-sets/x"),
    (Method::GET, "/admin/api/operation-rules"),
    (Method::POST, "/admin/api/operation-rules"),
    (Method::POST, "/admin/api/operation-rules/batch"),
    (Method::GET, "/admin/api/operation-rules/x"),
    (Method::PATCH, "/admin/api/operation-rules/x"),
    (Method::DELETE, "/admin/api/operation-rules/x"),
    (Method::GET, "/admin/api/operation-endpoints"),
    (Method::POST, "/admin/api/operation-endpoints"),
    (Method::POST, "/admin/api/operation-endpoints/batch"),
    (Method::GET, "/admin/api/operation-endpoints/x"),
    (Method::PATCH, "/admin/api/operation-endpoints/x"),
    (Method::DELETE, "/admin/api/operation-endpoints/x"),
    (Method::GET, "/admin/api/quotas"),
    (Method::POST, "/admin/api/quotas"),
    (Method::POST, "/admin/api/quotas/batch"),
    (Method::GET, "/admin/api/quotas/x"),
    (Method::PATCH, "/admin/api/quotas/x"),
    (Method::DELETE, "/admin/api/quotas/x"),
    (Method::GET, "/admin/api/quotas/status"),
    (Method::POST, "/admin/api/quotas/x/reset"),
    (Method::POST, "/admin/api/quotas/x/limit-reset"),
    (Method::GET, "/admin/api/price-rules"),
    (Method::POST, "/admin/api/price-rules"),
    (Method::POST, "/admin/api/price-rules/batch"),
    (Method::GET, "/admin/api/price-rules/x"),
    (Method::PATCH, "/admin/api/price-rules/x"),
    (Method::DELETE, "/admin/api/price-rules/x"),
    (Method::GET, "/admin/api/price-rates"),
    (Method::POST, "/admin/api/price-rates"),
    (Method::POST, "/admin/api/price-rates/batch"),
    (Method::GET, "/admin/api/price-rates/x"),
    (Method::PATCH, "/admin/api/price-rates/x"),
    (Method::DELETE, "/admin/api/price-rates/x"),
    (Method::GET, "/admin/api/price-tiers"),
    (Method::POST, "/admin/api/price-tiers"),
    (Method::POST, "/admin/api/price-tiers/batch"),
    (Method::GET, "/admin/api/price-tiers/x"),
    (Method::PATCH, "/admin/api/price-tiers/x"),
    (Method::DELETE, "/admin/api/price-tiers/x"),
    (Method::POST, "/admin/api/export"),
    (Method::POST, "/admin/api/import"),
    (Method::POST, "/admin/api/connectivity/test"),
    (Method::GET, "/admin/api/channels"),
    (Method::GET, "/admin/api/tls-presets"),
    (Method::GET, "/admin/api/rule-presets"),
    (Method::GET, "/admin/api/default-model-catalog"),
    (Method::GET, "/admin/api/model-names"),
    (Method::GET, "/admin/api/operation-keys"),
    (
        Method::POST,
        "/admin/api/default-model-catalog/apply-prices",
    ),
    (Method::GET, "/admin/api/tokenizer-vocabs"),
    (Method::POST, "/admin/api/tokenizer-vocabs"),
    (Method::GET, "/admin/api/tokenizer-vocabs/progress"),
    (Method::DELETE, "/admin/api/tokenizer-vocabs/x"),
    (Method::GET, "/admin/api/tokenizer-auth"),
    (Method::PATCH, "/admin/api/tokenizer-auth"),
    (Method::POST, "/admin/api/tokenizer-auth/reveal"),
];

/// Every route in the table exists, with the method it claims.
///
/// Two requests per route, each answering a different question, and neither
/// of them needing a row to exist:
///
/// * **without a credential → 401.** The guard is a `route_layer`, so it runs
///   only for a path the router matched; an unmatched `/admin/api/*` path
///   falls through to the ingress fallback and is a 404. A 401 therefore says
///   "this path is on the guarded surface" and nothing else — a typo cannot
///   pass it.
/// * **with the administrator's key → not 405.** The method is declared.
///   Anything else (a missing row, a body this request did not send, an
///   upstream that was never asked) is fine here; only "method not allowed"
///   means the route table is wrong.
#[tokio::test]
async fn every_configuration_route_is_reachable() {
    let host = instance().await;
    for (method, path) in TABLE {
        let anonymous = host.send(request(method.clone(), path)).await;
        assert_eq!(
            anonymous.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} did not reach the guarded surface: {}",
            anonymous.text()
        );
        if *path == "/admin/api/models/openrouter" {
            host.client.push(support::Reply::Http(
                StatusCode::OK,
                json!({"data": [{
                    "id": "vendor/remote-model", "name": "Remote model", "context_length": 32000,
                    "architecture": {"input_modalities": ["text", "image"]},
                    "top_provider": {"max_completion_tokens": 4096}
                }]}),
            ));
        }
        let authenticated = host.send(keyed(request(method.clone(), path), KEY)).await;
        if *path == "/admin/api/models/openrouter" {
            assert_eq!(authenticated.status, StatusCode::OK);
            let rows = authenticated.json();
            assert_eq!(rows[0]["upstreamName"], "remote-model");
            assert_eq!(rows[0]["metadata"]["context_window"], 32000);
            assert_eq!(rows[0]["metadata"]["max_output_tokens"], 4096);
            assert_eq!(
                rows[0]["metadata"]["input_modalities"],
                json!(["text", "image"])
            );
        }
        assert_ne!(
            authenticated.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is not declared with that method"
        );
    }
}

#[tokio::test]
async fn an_unknown_configuration_path_is_a_404_and_not_a_401() {
    let host = instance().await;
    // The counterpart of the table above: without it, a 401 would prove
    // nothing.
    for path in [
        "/admin/api/provider",
        "/admin/api/price-tier",
        "/admin/api/rule-sets/x/preset",
        "/admin/api/tokenizer",
    ] {
        let answer = host.send(get(path)).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn global_network_and_instance_info_follow_settings_without_restart() {
    let host = instance().await;
    let saved = call(
        &host,
        Method::PATCH,
        "/admin/api/settings",
        json!({"instance": {
            "instanceName": "Live gateway", "corsOrigins": ["https://client.example/"],
            "trustedProxies": ["192.0.2.10"]
        }}),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    host.publish().await;
    let info = host.send(get("/info")).await.json();
    assert_eq!(info["instanceName"], "Live gateway");
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert!(!info["hash"].as_str().unwrap().is_empty());
    let mut request = get("/v1/models");
    request
        .headers_mut()
        .insert("origin", "https://client.example".parse().unwrap());
    let response = host.send(request).await;
    assert_eq!(
        response.headers["access-control-allow-origin"],
        "https://client.example"
    );
    assert_eq!(
        gproxy_host_axum::runtime_settings::trusted_proxies(&host.app),
        ["192.0.2.10"]
    );
    let saved = call(
        &host,
        Method::PATCH,
        "/admin/api/settings",
        json!({"instance": {"corsOrigins": [], "trustedProxies": []}}),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    host.publish().await;
    assert!(gproxy_host_axum::runtime_settings::cors_origins(&host.app).is_empty());
}
