//! The command table, and the handful of commands whose shape it cannot
//! express.
//!
//! This file is meant to be read as a document. Everything above the `@manual`
//! block is a declaration — surface, family, receiver, methods — and the
//! `#[tauri::command]` functions, the [`OPERATIONS`] inventory and the
//! [`invoke_handler`] dispatch table are all generated from it, so the three
//! cannot disagree about what this host binds.
//!
//! Below the declaration are the exceptions, written out. Each one is there
//! for a reason the grammar deliberately does not accommodate: an argument
//! that is a slice, a clock the webview must not be trusted with, a
//! synchronous accessor that answers no `Result`. Inventing table syntax for
//! each of those would make the common case — which is nine commands in ten —
//! harder to read in order to shorten the rare one.
//!
//! # The families that are not here
//!
//! - **the OAuth issuer** (`Operations::issuer`). It is the authorization
//!   server this instance runs *for downstream clients*, reached at
//!   `/v1/oauth/*` by programs that redirect a browser at it. There is no
//!   meaning to invoking `token` over the IPC bridge of the application that
//!   is issuing it, and the embedded data plane already serves the real one.
//! - **the data plane itself** (`App::call`, `App::connect`). Its clients
//!   speak HTTP and cannot speak IPC; that is the premise of this whole crate.
//! - **`Operations::portal`**, which is a constructor rather than a family:
//!   every `portal_*` command below goes through it.
//! - **`Operations::portal_login` and `portal_logout`**, which establish a
//!   browser session. There is nothing to establish here: the IPC channel is
//!   the trust boundary and `Desktop::caller` is the local administrator.
//!
//! `tests/table.rs` names each of those with its reason, and fails if a
//! family appears in `gproxy-app` or `gproxy-sdk` without a command here.

use gproxy_app::dto as app;
use gproxy_core::{BudgetOwner, RefreshMode};
use gproxy_sdk::dto as sdk;

use super::{Operation, ipc_call, ipc_table, json};
use crate::{Desktop, IpcResult};

ipc_table! {
    // ---------------------------------------------------------- admin --
    // The operator's surface: `gproxy_app::Operations`, one family per
    // accessor, and the same five-method shape the HTTP host routes.

    admin users [.users()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::UserWrite);
        update[id](patch: app::UserPatch);
        delete[id];
        batch(items: Vec<app::BatchItem<app::UserWrite, app::UserPatch>>);
        set_password[id, new_password];
        clear_password[id];
        set_allowlist[id](clients: Option<Vec<String>>);
    }

    admin api_keys [.api_keys()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::ApiKeyWrite);
        update[id](patch: app::ApiKeyPatch);
        delete[id];
        rotate[id];
        reveal[id];
    }

    admin organizations [.organizations()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::OrganizationWrite);
        update[id](patch: app::OrganizationPatch);
        delete[id];
        batch(items: Vec<app::BatchItem<app::OrganizationWrite, app::OrganizationPatch>>);
    }

    admin teams [.teams()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::TeamWrite);
        update[id](patch: app::TeamPatch);
        delete[id];
        batch(items: Vec<app::BatchItem<app::TeamWrite, app::TeamPatch>>);
    }

    // Membership is a composite key, so it is addressed by both halves
    // rather than by an id — the same reason the HTTP host gives it its own
    // routes instead of the family shape.
    admin members [.members()] {
        list(query: app::ListQuery);
        get[organization_id, user_id];
        add[organization_id](write: app::MemberWrite);
        set_role[organization_id, user_id](patch: app::MemberPatch);
        remove[organization_id, user_id];
    }

    admin team_members [.team_members()] {
        list(query: app::ListQuery);
        get[team_id, user_id];
        add[team_id](write: app::MemberWrite);
        set_role[team_id, user_id](patch: app::MemberPatch);
        remove[team_id, user_id];
    }

    admin permissions [.permissions()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::PermissionWrite);
        update[id](patch: app::PermissionPatch);
        delete[id];
        batch(items: Vec<app::BatchItem<app::PermissionWrite, app::PermissionPatch>>);
    }

    admin rate_limits [.rate_limits()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::RateLimitWrite);
        update[id](patch: app::RateLimitPatch);
        delete[id];
        batch(items: Vec<app::BatchItem<app::RateLimitWrite, app::RateLimitPatch>>);
    }






    // An OAuth client is retired, not deleted: the grants it issued still
    // name it, so the row survives and stops being usable.
    admin oauth_clients [.oauth_clients()] {
        list(query: app::ListQuery);
        get[id];
        create(write: app::OAuthClientWrite);
        update[id](patch: app::OAuthClientPatch);
        retire[id];
    }

    admin sessions [.sessions()] {
        list[user_id];
        page(query: app::ListQuery);
        revoke[id];
        revoke_all[user_id];
    }

    admin audit [.audit()] {
        query(query: app::AuditQuery);
    }

    // --------------------------------------------------------- portal --
    // The end user's surface. Every command is scoped to
    // `Desktop::caller` by construction — no method takes a user id — which
    // is exactly how the HTTP portal is safe for an ordinary account.

    portal me [] {
        context;
        usage(query: app::PortalUsageQuery);
        quota;
        recent_requests(limit: u64);
        sessions;
    }

    portal keys [.keys()] {
        list;
        create(write: app::PortalKeyCreate);
        rotate[id];
        reveal[id];
        delete[id];
    }

    portal oauth_sessions [.oauth_sessions()] {
        list;
        revoke[grant_id];
    }

    portal password [.password()] {
        change(change: app::PortalPasswordChange);
    }

    // --------------------------------------------------------- manage --
    // The engine's configuration families: `gproxy.manage()`.

    manage providers [.providers()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ProviderWrite);
        update[id](patch: sdk::ProviderPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ProviderWrite, sdk::ProviderPatch>>);
        reset_routing_defaults[provider_id];
    }

    manage credentials [.credentials()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::CredentialWrite);
        update[id](patch: sdk::CredentialPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::CredentialWrite, sdk::CredentialPatch>>);
        reveal_secret[id];
        quota_probe[id];
        quota_read[id];
        quota_reset[id];
        health_reset[id];
        limit_status[id];
    }

    manage models [.models()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ModelWrite);
        update[id](patch: sdk::ModelPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ModelWrite, sdk::ModelPatch>>);
    }

    manage provider_models [.provider_models()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ProviderModelWrite);
        update[id](patch: sdk::ProviderModelPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ProviderModelWrite, sdk::ProviderModelPatch>>);
    }

    manage routes [.routes()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::RouteWrite);
        update[id](patch: sdk::RoutePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::RouteWrite, sdk::RoutePatch>>);
    }

    manage route_members [.route_members()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::RouteMemberWrite);
        update[id](patch: sdk::RouteMemberPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::RouteMemberWrite, sdk::RouteMemberPatch>>);
    }

    manage exposed_models [.exposed_models()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ExposedModelWrite);
        update[id](patch: sdk::ExposedModelPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ExposedModelWrite, sdk::ExposedModelPatch>>);
    }

    manage connection_profiles [.connection_profiles()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ConnectionProfileWrite);
        update[id](patch: sdk::ConnectionProfilePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ConnectionProfileWrite, sdk::ConnectionProfilePatch>>);
    }

    manage settings [.settings()] {
        get;
        update(patch: sdk::SettingsPatch);
    }

    manage rewrite [.rewrite()] {
        replace_rules[set_id](rules: Vec<sdk::RewriteRuleWrite>);
    }

    manage rewrite_sets [.rewrite().sets()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::RuleSetWrite);
        update[id](patch: sdk::RuleSetPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::RuleSetWrite, sdk::RuleSetPatch>>);
    }

    manage rewrite_rules [.rewrite().rules()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::RewriteRuleWrite);
        update[id](patch: sdk::RewriteRulePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::RewriteRuleWrite, sdk::RewriteRulePatch>>);
    }

    manage rewrite_bindings [.rewrite().bindings()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::ProviderRuleSetWrite);
        update[id](patch: sdk::ProviderRuleSetPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::ProviderRuleSetWrite, sdk::ProviderRuleSetPatch>>);
    }

    manage endpoints_operation_rules [.endpoints().operation_rules()] {
        effective[id];
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::OperationRuleWrite);
        update[id](patch: sdk::OperationRulePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::OperationRuleWrite, sdk::OperationRulePatch>>);
    }

    manage endpoints_operation_endpoints [.endpoints().operation_endpoints()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::OperationEndpointWrite);
        update[id](patch: sdk::OperationEndpointPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::OperationEndpointWrite, sdk::OperationEndpointPatch>>);
    }

    manage quotas [.quotas()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::QuotaWrite);
        update[id](patch: sdk::QuotaPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::QuotaWrite, sdk::QuotaPatch>>);
        reset_budget[quota_id];
        limit_status[credential_id];
        reset_limit[quota_id];
    }

    manage pricing_rules [.pricing().rules()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::PriceRuleWrite);
        update[id](patch: sdk::PriceRulePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::PriceRuleWrite, sdk::PriceRulePatch>>);
    }

    manage pricing_rates [.pricing().rates()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::PriceRateWrite);
        update[id](patch: sdk::PriceRatePatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::PriceRateWrite, sdk::PriceRatePatch>>);
    }

    manage pricing_tiers [.pricing().tiers()] {
        list(query: sdk::ListQuery);
        get[id];
        create(write: sdk::PriceTierWrite);
        update[id](patch: sdk::PriceTierPatch);
        delete[id];
        batch(items: Vec<sdk::BatchItem<sdk::PriceTierWrite, sdk::PriceTierPatch>>);
    }

    manage transfer [.transfer()] {
        export(request: sdk::ExportRequest);
        import(request: sdk::ImportRequest);
    }

    manage catalog [.catalog()] {
        apply_default_prices(request: sdk::ApplyDefaultPricesRequest);
        apply_rule_preset(request: sdk::ApplyRulePreset);
    }

    manage connectivity [.connectivity()] {
        test(request: sdk::ConnectivityTest);
        model_test(request: sdk::ModelTest);
        apply_discovered[provider_id](upstream_names: Vec<String>);
    }

    manage tokenizer [.tokenizer()] {
        vocabularies;
        fetch(request: sdk::TokenizerFetch);
        delete[file_id];
        auth;
        set_auth(token: Option<String>);
        reveal_auth;
    }

    // ---------------------------------------------------------- query --
    // Read-only history: `gproxy.query()`.

    query usage [.usage()] {
        records(query: sdk::UsageRecordQuery);
        summary(query: sdk::UsageQuery);
        group(query: sdk::UsageGroupQuery);
        trend(query: sdk::UsageTrendQuery);
    }

    query quota [.quota()] {
        windows(query: sdk::QuotaWindowQuery);
        settlements[window_id](query: sdk::ListQuery);
        credential_cycles[credential_id];
    }

    query logs [.logs()] {
        list(query: sdk::LogQuery);
        detail[request_id];
    }

    // ------------------------------------------------------- upstream --
    // The three upstream login flows: `gproxy.login()`. This is the surface a
    // desktop shell exists for — an authorization-code login needs a browser
    // and a loopback callback, which is what a window has and a headless
    // server does not.
    //
    // `upstream` rather than `login` because the portal has a login too, and
    // that one is a person signing in to this instance. These create a
    // credential for somebody else's API.

    upstream login [.login()] {
        authcode_start(request: sdk::AuthCodeStart);
        authcode_complete(request: sdk::AuthCodeComplete);
        device_start(request: sdk::DeviceStart);
        device_poll[login_session_id];
        cookie_exchange(request: sdk::CookieExchange);
    }

    @manual {
        // A synchronous accessor with no `Result`: the compiled-in channel
        // descriptors, the TLS presets and the rewrite presets are constants
        // of the build, not reads.
        manage catalog channels => manage_catalog_channels;
        manage catalog default_models => manage_catalog_default_models;
        manage catalog tls_presets => manage_catalog_tls_presets;
        manage catalog rule_presets => manage_catalog_rule_presets;
        // Returns `Option`, and is a progress poll rather than an operation.
        manage tokenizer progress => manage_tokenizer_progress;
        // `RefreshMode` is an engine enum with no `Deserialize`, so the
        // argument is the one bit the caller actually chooses.
        manage credentials refresh => manage_credentials_refresh;
        // Two owned arguments, one of which is an enum the store owns.
        manage credentials set_status => manage_credentials_set_status;
        // A slice argument.
        manage quotas budget_status => manage_quotas_budget_status;
        // `Option<&str>`.
        manage connectivity discover_models => manage_connectivity_discover_models;
        // Synchronous, and the only `Portal` method that is.
        portal me models => portal_me_models;
        // These take `now_ms`. A clock is the host's to supply: a webview that
        // could choose "now" could read a budget window that has not started
        // or purge sessions that have not expired.
        query quota counted_windows => query_quota_counted_windows;
        query quota budget_status => query_quota_budget_status;
        admin sessions purge_expired => admin_sessions_purge_expired;
        // The shell's own state. A surface of its own, `desktop`, because it
        // is the only thing here that is not an operation on the instance:
        // where the socket is listening and which store the secrets went to
        // are facts about this process.
        desktop instance status => desktop_instance_status;
        desktop instance gateway_key => desktop_instance_gateway_key;
        desktop instance reload => desktop_instance_reload;
    }
}

// --------------------------------------------------------------- manual --

/// Every channel compiled into this build, as a provider form is rendered
/// from.
#[tauri::command]
pub async fn manage_catalog_channels(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(Ok::<_, gproxy_sdk::SdkError>(
        desktop.app().gproxy().manage().catalog().channels(),
    ))
}

/// The bundled default model catalog.
#[tauri::command]
pub async fn manage_catalog_default_models(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(desktop.app().gproxy().manage().catalog().default_models())
}

/// The named TLS/fingerprint presets a connection profile can start from.
#[tauri::command]
pub async fn manage_catalog_tls_presets(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(Ok::<_, gproxy_sdk::SdkError>(
        desktop.app().gproxy().manage().catalog().tls_presets(),
    ))
}

/// The named rewrite-rule presets.
#[tauri::command]
pub async fn manage_catalog_rule_presets(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(Ok::<_, gproxy_sdk::SdkError>(
        desktop.app().gproxy().manage().catalog().rule_presets(),
    ))
}

/// How far a vocabulary download has got, or `null` when none is running.
#[tauri::command]
pub async fn manage_tokenizer_progress(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(Ok::<_, gproxy_sdk::SdkError>(
        desktop.app().gproxy().manage().tokenizer().progress(),
    ))
}

/// Refresh a credential. `force` is [`RefreshMode`] as a caller experiences
/// it: refresh now, or only if it is due.
#[tauri::command]
pub async fn manage_credentials_refresh(
    desktop: tauri::State<'_, Desktop>,
    id: String,
    force: Option<bool>,
) -> IpcResult<serde_json::Value> {
    let mode = if force.unwrap_or(false) {
        RefreshMode::Force
    } else {
        RefreshMode::IfNeeded
    };
    json(
        desktop
            .app()
            .gproxy()
            .manage()
            .credentials()
            .refresh(&id, mode)
            .await,
    )
}

/// Move a credential's lifecycle status, with the reason that goes in the row.
#[tauri::command]
pub async fn manage_credentials_set_status(
    desktop: tauri::State<'_, Desktop>,
    id: String,
    status: gproxy_store::entity::upstream::credential::CredentialStatus,
    reason: Option<String>,
) -> IpcResult<serde_json::Value> {
    json(
        desktop
            .app()
            .gproxy()
            .manage()
            .credentials()
            .set_status(&id, status, reason)
            .await,
    )
}

/// Where a chain of budget owners stands.
#[tauri::command]
pub async fn manage_quotas_budget_status(
    desktop: tauri::State<'_, Desktop>,
    owners: Vec<BudgetOwner>,
) -> IpcResult<serde_json::Value> {
    json(
        desktop
            .app()
            .gproxy()
            .manage()
            .quotas()
            .budget_status(&owners)
            .await,
    )
}

/// Ask a provider what models it has, without writing anything.
#[tauri::command]
pub async fn manage_connectivity_discover_models(
    desktop: tauri::State<'_, Desktop>,
    provider_id: String,
    credential_id: Option<String>,
) -> IpcResult<serde_json::Value> {
    json(
        desktop
            .app()
            .gproxy()
            .manage()
            .connectivity()
            .discover_models(&provider_id, credential_id.as_deref())
            .await,
    )
}

/// The models this account may call, as the portal lists them.
#[tauri::command]
pub async fn portal_me_models(desktop: tauri::State<'_, Desktop>) -> IpcResult<serde_json::Value> {
    let data = desktop.app().data();
    let operations = desktop.operations(&data);
    json(operations.portal(desktop.caller()).models())
}

/// The quota windows a credential is currently counted against.
#[tauri::command]
pub async fn query_quota_counted_windows(
    desktop: tauri::State<'_, Desktop>,
    credential_id: String,
) -> IpcResult<serde_json::Value> {
    json(
        desktop
            .app()
            .gproxy()
            .query()
            .quota()
            .counted_windows(&credential_id, crate::now_ms())
            .await,
    )
}

/// The same budget question as the management family, read rather than acted
/// on.
#[tauri::command]
pub async fn query_quota_budget_status(
    desktop: tauri::State<'_, Desktop>,
    owners: Vec<BudgetOwner>,
) -> IpcResult<serde_json::Value> {
    json(
        desktop
            .app()
            .gproxy()
            .query()
            .quota()
            .budget_status(&owners, crate::now_ms())
            .await,
    )
}

/// Drop every console session whose lifetime has run out.
#[tauri::command]
pub async fn admin_sessions_purge_expired(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    let data = desktop.app().data();
    let operations = desktop.operations(&data);
    json(operations.sessions().purge_expired(crate::now_ms()).await)
}

// ------------------------------------------------------------- the shell --

/// What the window renders its header and its warning banner from.
///
/// Deliberately **without** the gateway key. A status call is the thing a
/// console makes on every navigation, and a secret that rides along on every
/// call is a secret in every log, every error report and every screenshot of a
/// developer tools panel. Ask for it when you are about to show it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    /// The loopback address external clients point at.
    pub base_url: String,
    pub port: u16,
    /// The configuration revision this process is serving, the same number
    /// `/healthz` reports.
    pub revision: i64,
    /// `false` means upstream credentials are stored in the clear, and the
    /// window is expected to say so where a person will see it.
    pub secrets_are_sealed: bool,
    pub secrets: crate::desktop::SecretPlacement,
    /// Where the database, the configuration file and the local file storage
    /// are, for a "reveal in file manager" button.
    pub data_dir: String,
    /// The file an instance is configured through, whether or not it exists.
    pub config_file: String,
}

#[tauri::command]
pub async fn desktop_instance_status(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    let plane = desktop.data_plane();
    json(Ok::<_, gproxy_app::AppError>(InstanceStatus {
        base_url: plane.base_url.clone(),
        port: plane.address.port(),
        revision: desktop.app().snapshot().revision(),
        secrets_are_sealed: desktop.secrets().secrets_are_sealed(),
        secrets: desktop.secrets(),
        data_dir: desktop.data_dir().display().to_string(),
        config_file: desktop
            .data_dir()
            .join(crate::config::CONFIG_FILE)
            .display()
            .to_string(),
    }))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayKey {
    pub base_url: String,
    /// The key a client puts in `Authorization: Bearer …`.
    pub token: String,
}

/// The gateway key, for the moment a person is copying it into a client.
#[tauri::command]
pub async fn desktop_instance_gateway_key(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    let plane = desktop.data_plane();
    json(Ok::<_, gproxy_app::AppError>(GatewayKey {
        base_url: plane.base_url.clone(),
        token: plane.gateway_key.clone(),
    }))
}

/// Re-read the engine's snapshot and this instance's identity from the
/// database, and answer with the revision that was read.
///
/// Needed because a desktop instance can share a database with a `gproxy
/// serve` on the same machine — a developer running both — and because a
/// restore from a backup happens under the running process.
#[tauri::command]
pub async fn desktop_instance_reload(
    desktop: tauri::State<'_, Desktop>,
) -> IpcResult<serde_json::Value> {
    json(desktop.app().reload_all().await)
}
