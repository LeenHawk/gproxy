//! The identity half: users, keys, scopes, permissions and rate limits.
//!
//! None of this is in the sdk's reach — identity is `gproxy-app`'s — so every
//! row goes through [`crate::Operations`], the same families the admin API
//! calls, exactly as the host bootstrap does. That is not a stylistic
//! preference. The binding rules that say a key's organization must exist and
//! that its holder must be a member of it live in those families and nowhere
//! else; a hand-rolled insert would be a second door past them, which is
//! precisely the mistake v3 made with its bootstrap key.
//!
//! # The key digest is the whole point
//!
//! v3 stored `SHA-256(payload)`, the key text with an `sk-`/`at-` presentation
//! prefix removed (`v3:crates/gproxy-app/src/control/user_key.rs`). v4's
//! [`crate::auth::digests`] probes the raw digest of the presented token
//! *and* that same prefix-stripped one, the second rung existing because "it is
//! what v3 wrote". So a migrated row carries v3's digest bytes verbatim and the
//! operator's existing key keeps authenticating — which is the single fact that
//! decides whether this migration is worth running.
//!
//! A `digest_version` other than `1` is refused loudly. Only version 1 has ever
//! existed, and a row written under a rule this build does not know is a key
//! that can never match anything.
//!
//! # Two-pass, and why
//!
//! Every write is `create` when the id is new and `update` when it is not, so a
//! re-run of an interrupted import completes it instead of failing on the first
//! row it already made. The ids are deterministic ([`ids`]), which is what makes
//! that possible.
//!
//! # What v3 had that v4 does not
//!
//! - **`is_admin` on a user** becomes the instance role `admin`; v4's
//!   organization and team *memberships* carry their own roles, and v3 had no
//!   such thing, so every migrated membership is a plain `member`.
//! - **A permission or rate limit on an organization or a team.** v4 addresses
//!   exactly one subject, a user or a key, because a rule with both or neither
//!   is a rule nobody can reason about. An org-wide v3 rule has no v4 row and
//!   is reported.
//! - **`operation_group`.** v3 grouped operations behind a name; v4's
//!   permission names one `gproxy_protocol::Operation` or every one of them.
//!   A grouped rule becomes one rule per operation of its group (see
//!   [`group_operations`]); widening it to every operation would turn a
//!   narrow deny into a blanket one.
//! - **Passwords.** SQLite imports preserve v3's Argon2 PHC hashes. JSON exports
//!   without hashes can use [`super::admin_password`] to set one.

use std::{collections::HashSet, sync::Arc};

use crate::{
    App, AppError, Operations,
    dto::{
        ApiKeyWrite, MemberWrite, OAuthClientWrite, OrganizationWrite, PermissionWrite,
        RateLimitWrite, TeamWrite, UserWrite,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::{api_key, organization, permission, team, user};
use gproxy_store::entity::limits::rate_limit;
use gproxy_store::entity::oauth::client as oauth_client;
use sea_orm::EntityTrait;

use super::{Error, Result};
use super::{
    Report,
    document::{self, DIGEST_VERSION, Document},
    ids,
    secret::{Bridge, Domain},
};

/// v4's instance roles.
const ADMIN: &str = "admin";
const USER: &str = "user";

/// What a migrated key is shown as in a list when v3 kept no copy of its text.
/// v4 normally shows the first eight characters of the key body; v3 stored only
/// the presentation prefix, so that is what there is.
const UNKNOWN_PREFIX: &str = "v3";

/// Write the identity half. Returns what landed and what did not.
///
/// `bridge` is used only for the keys v3 was told to keep revealable: their
/// text is in the document, sealed, and carrying it across means v4's `reveal`
/// keeps answering for them.
pub async fn write<C>(
    app: &Arc<App<C>>,
    document: &Document,
    bridge: &Bridge,
    dropped_providers: &std::collections::BTreeSet<i64>,
) -> Result<Report>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut report = Report::default();
    let data = &document.data;

    organizations(app, data, &mut report).await?;
    teams(app, data, &mut report).await?;
    // Users and their memberships have to land before any key names them: the
    // key family checks both, and a snapshot one revision behind would refuse a
    // binding that is in fact valid.
    let users = users(app, data, &mut report).await?;
    app.reload_all().await?;
    memberships(app, data, &users, &mut report).await?;
    app.reload_all().await?;

    let keys = api_keys(app, data, &users, bridge, &mut report).await?;
    app.reload_all().await?;

    permissions(app, data, &users, &keys, dropped_providers, &mut report).await?;
    rate_limits(app, data, &users, &keys, &mut report).await?;
    oauth_clients(app, data, &mut report).await?;
    app.reload_all().await?;

    Ok(report)
}

// --------------------------------------------------------------- scopes --

async fn organizations<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().organizations()).await?;
    let mut written = 0;
    for row in &data.organizations {
        let id = ids::id("organizations", row.id);
        let write = OrganizationWrite {
            id: Some(id.clone()),
            name: row.name.clone(),
            oauth_client_allowlist: None,
        };
        if !row.enabled {
            report.warn(format!(
                "organization {} ({}) was disabled in v3; v4 organizations have no enabled \
                 column, so it arrives active",
                row.id, row.name
            ));
        }
        let data = app.data();
        let operations = Operations::new(app.gproxy(), &data, app.config());
        if present.contains(&id) {
            operations
                .organizations()
                .update(
                    &id,
                    crate::dto::OrganizationPatch {
                        name: Some(row.name.clone()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| named("organization", row.id, error))?;
        } else {
            operations
                .organizations()
                .create(write)
                .await
                .map_err(|error| named("organization", row.id, error))?;
        }
        written += 1;
    }
    report.count("organizations", written);
    Ok(())
}

async fn teams<C>(app: &Arc<App<C>>, data: &document::Data, report: &mut Report) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().teams()).await?;
    let mut written = 0;
    for row in &data.teams {
        let id = ids::id("teams", row.id);
        if !row.enabled {
            report.warn(format!(
                "team {} ({}) was disabled in v3; v4 teams have no enabled column, so it \
                 arrives active",
                row.id, row.name
            ));
        }
        let data = app.data();
        let operations = Operations::new(app.gproxy(), &data, app.config());
        if present.contains(&id) {
            operations
                .teams()
                .update(
                    &id,
                    crate::dto::TeamPatch {
                        name: Some(row.name.clone()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| named("team", row.id, error))?;
        } else {
            operations
                .teams()
                .create(TeamWrite {
                    id: Some(id),
                    organization_id: ids::id("organizations", row.organization_id),
                    name: row.name.clone(),
                    oauth_client_allowlist: None,
                })
                .await
                .map_err(|error| named("team", row.id, error))?;
        }
        written += 1;
    }
    report.count("teams", written);
    Ok(())
}

// ---------------------------------------------------------------- users --

async fn users<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    report: &mut Report,
) -> Result<HashSet<String>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().users()).await?;
    let mut landed = HashSet::new();
    for row in &data.users {
        let id = ids::id("users", row.id);
        let role = if row.is_admin { ADMIN } else { USER };
        let data = app.data();
        let operations = Operations::new(app.gproxy(), &data, app.config());
        if present.contains(&id) {
            operations
                .users()
                .update(
                    &id,
                    crate::dto::UserPatch {
                        name: Some(row.name.clone()),
                        role: Some(role.to_owned()),
                        enabled: Some(row.enabled),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| named("user", row.id, error))?;
        } else {
            operations
                .users()
                .create(UserWrite {
                    id: Some(id.clone()),
                    name: row.name.clone(),
                    // v3's export carries no password hash and v4 stores an
                    // argon2 hash it cannot forge from nothing. The user
                    // arrives able to use its API keys and unable to sign in.
                    password: None,
                    role: Some(role.to_owned()),
                    enabled: Some(row.enabled),
                    oauth_client_allowlist: None,
                })
                .await
                .map_err(|error| named("user", row.id, error))?;
        }
        if let Some(hash) = &row.password_hash {
            use sea_orm::ActiveValue::Set;
            app.gproxy()
                .store()
                .users()
                .update_many(vec![user::ActiveModel {
                    id: Set(id.clone()),
                    password_hash: Set(Some(hash.clone())),
                    ..Default::default()
                }])
                .await?;
        }
        landed.insert(id);
    }
    report.count("users", landed.len() as u64);
    if data.users.iter().any(|user| user.password_hash.is_none()) {
        report.warn(
            "some imported users have no console password: v3's JSON export does not carry password \
             hashes. Pass --admin-password to give the v3 administrator one, and reset the \
             rest from the console."
                .to_owned(),
        );
    }
    Ok(landed)
}

/// v3 put a user's organization and team on the user row; v4 has membership
/// tables, because a user can be in more than one. One v3 user becomes at most
/// one organization membership and one team membership, both as plain members:
/// v3 had no per-scope role to carry.
async fn memberships<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    users: &HashSet<String>,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let organizations: HashSet<i64> = data.organizations.iter().map(|row| row.id).collect();
    let teams: HashSet<i64> = data.teams.iter().map(|row| row.id).collect();
    let mut written = 0;
    for row in &data.users {
        let user_id = ids::id("users", row.id);
        if !users.contains(&user_id) {
            continue;
        }
        if let Some(organization_id) = row.organization_id {
            if !organizations.contains(&organization_id) {
                report.drop_row(
                    "users.organization_id",
                    format!("user {} ({})", row.id, row.name),
                    format!("it named organization {organization_id}, which is not in the export"),
                );
            } else {
                let data = app.data();
                let operations = Operations::new(app.gproxy(), &data, app.config());
                match operations
                    .members()
                    .add(
                        &ids::id("organizations", organization_id),
                        MemberWrite {
                            user_id: user_id.clone(),
                            role: None,
                        },
                    )
                    .await
                {
                    Ok(_) => written += 1,
                    // Already a member: a re-run, not a failure.
                    Err(AppError::Conflict(_)) => {}
                    Err(error) => return Err(named("organization member", row.id, error)),
                }
            }
        }
        if let Some(team_id) = row.team_id {
            if !teams.contains(&team_id) {
                report.drop_row(
                    "users.team_id",
                    format!("user {} ({})", row.id, row.name),
                    format!("it named team {team_id}, which is not in the export"),
                );
            } else {
                let data = app.data();
                let operations = Operations::new(app.gproxy(), &data, app.config());
                match operations
                    .team_members()
                    .add(
                        &ids::id("teams", team_id),
                        MemberWrite {
                            user_id: user_id.clone(),
                            role: None,
                        },
                    )
                    .await
                {
                    Ok(_) => written += 1,
                    Err(AppError::Conflict(_)) => {}
                    Err(error) => return Err(named("team member", row.id, error)),
                }
            }
        }
    }
    report.count("memberships", written);
    Ok(())
}

// ----------------------------------------------------------------- keys --

async fn api_keys<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    users: &HashSet<String>,
    bridge: &Bridge,
    report: &mut Report,
) -> Result<HashSet<String>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().api_keys()).await?;
    let mut landed = HashSet::new();
    for row in &data.user_keys {
        let id = ids::id("user_keys", row.config.id);
        // Refused, not skipped: a key row under a digest rule this build does
        // not implement can never match a presented token, and importing one
        // would look exactly like a working migration until someone tried it.
        if row.digest_version != DIGEST_VERSION {
            return Err(Error::other(format!(
                "user key {} uses digest version {}, and this build only knows version \
                 {DIGEST_VERSION}. A row written under an unknown digest rule could never \
                 authenticate, so the import is refused rather than left silently broken.",
                row.config.id, row.digest_version
            )));
        }
        let digest: [u8; 32] = row.digest.as_slice().try_into().map_err(|_| {
            Error::other(format!(
                "user key {} has a {}-byte digest where 32 were expected; the export is damaged",
                row.config.id,
                row.digest.len()
            ))
        })?;

        let user_id = ids::id("users", row.config.user_id);
        if !users.contains(&user_id) {
            report.drop_row(
                "user_keys",
                format!("key {} ({})", row.config.id, label(&row.config)),
                format!(
                    "it belongs to user {}, which is not in the export",
                    row.config.user_id
                ),
            );
            continue;
        }

        // The key text, when v3 was told to keep one revealable. Its absence is
        // normal and not worth a warning: v4 defaults to not retaining either.
        let retained = match &row.secret {
            Some(envelope) => bridge
                .open(Domain::UserKey, &id, envelope)?
                .as_str()
                .map(str::to_owned),
            None => None,
        };
        let prefix = retained
            .as_deref()
            .map(display_prefix)
            .or_else(|| {
                row.config
                    .prefix
                    .as_deref()
                    .map(str::trim)
                    .filter(|prefix| !prefix.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| UNKNOWN_PREFIX.to_owned());

        let write = ApiKeyWrite {
            id: Some(id.clone()),
            user_id: user_id.clone(),
            name: label(&row.config),
            // v3 keys were not bound to a scope: its budgets and permissions
            // addressed the key itself.
            organization_id: None,
            team_id: None,
            expires_at_ms: row.config.expires_at,
            enabled: Some(row.config.enabled),
            retain_secret: None,
            // A carried key calls models; management access is granted in the
            // console, where the operator can see which key it goes to.
            management: None,
            budget: None,
        };
        let app_data = app.data();
        let operations = Operations::new(app.gproxy(), &app_data, app.config());
        if present.contains(&id) {
            // A re-run: the digest is already there and is the one thing that
            // must not change, so only the row's description is refreshed.
            operations
                .api_keys()
                .update(
                    &id,
                    crate::dto::ApiKeyPatch {
                        name: Some(write.name.clone()),
                        expires_at_ms: Some(write.expires_at_ms),
                        enabled: write.enabled,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| named("api key", row.config.id, error))?;
        } else {
            operations
                .api_keys()
                .adopt_digest(write, &digest, &prefix, retained.as_deref())
                .await
                .map_err(|error| named("api key", row.config.id, error))?;
        }
        landed.insert(id);
    }
    report.count("api_keys", landed.len() as u64);
    Ok(landed)
}

fn label(config: &document::UserKeyConfig) -> String {
    config
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("v3 key {}", config.id))
}

/// The first eight characters of the key body, matching what a v4-minted key
/// shows in a list.
fn display_prefix(token: &str) -> String {
    let body = token
        .strip_prefix("sk-")
        .or_else(|| token.strip_prefix("at-"))
        .unwrap_or(token);
    match body.chars().take(8).collect::<String>() {
        prefix if prefix.is_empty() => UNKNOWN_PREFIX.to_owned(),
        prefix => prefix,
    }
}

// ---------------------------------------------------- permissions, limits --

async fn permissions<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    users: &HashSet<String>,
    keys: &HashSet<String>,
    dropped_providers: &std::collections::BTreeSet<i64>,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().permissions()).await?;
    let mut written = 0;
    for row in &data.permissions {
        let named_row = format!(
            "permission {} ({}:{})",
            row.id, row.subject_kind, row.subject_id
        );
        if row
            .provider_id
            .is_some_and(|id| dropped_providers.contains(&id))
        {
            report.drop_row(
                "permissions",
                named_row,
                "its provider was left behind by --skip-unmappable-providers",
            );
            continue;
        }
        let Some(subject) = subject(&row.subject_kind, row.subject_id, users, keys) else {
            report.drop_row(
                "permissions",
                named_row,
                unmappable_subject(&row.subject_kind),
            );
            continue;
        };
        // v3's group, as the v4 operations it covered; no group is every one.
        let operations: Vec<Option<&str>> = match row.operation_group.as_deref().map(str::trim) {
            None | Some("") => vec![None],
            Some(group) => match group_operations(group) {
                Some(operations) => operations.iter().copied().map(Some).collect(),
                None => {
                    report.drop_row(
                        "permissions",
                        named_row,
                        format!("its operation group `{group}` is not one v3 defined"),
                    );
                    continue;
                }
            },
        };
        // v3's null meant "every model"; v4 spells that `*`.
        let model_pattern = row
            .model_pattern
            .as_deref()
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .unwrap_or("*")
            .to_owned();
        let action = if row.allowed { "allow" } else { "deny" }.to_owned();
        let app_data = app.data();
        let ops = Operations::new(app.gproxy(), &app_data, app.config());
        for operation in operations {
            let id = match operation {
                None => ids::id("permissions", row.id),
                Some(operation) => ids::part("permissions", row.id, operation),
            };
            if present.contains(&id) {
                ops.permissions()
                    .update(
                        &id,
                        crate::dto::PermissionPatch {
                            action: Some(action.clone()),
                            model_pattern: Some(model_pattern.clone()),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|error| named("permission", row.id, error))?;
            } else {
                ops.permissions()
                    .create(PermissionWrite {
                        id: Some(id),
                        user_id: subject.user_id.clone(),
                        api_key_id: subject.api_key_id.clone(),
                        provider_id: row.provider_id.map(|id| ids::id("providers", id)),
                        model_pattern: Some(model_pattern.clone()),
                        operation: operation.map(str::to_owned),
                        action: action.clone(),
                        priority: None,
                    })
                    .await
                    .map_err(|error| named("permission", row.id, error))?;
            }
            written += 1;
        }
    }
    report.count("permissions", written);
    Ok(())
}

/// The OAuth clients v3's issuer registered, under the same client ids so a
/// client configured against v3 keeps working. A retired client stays
/// retired by not being written; one the destination already has keeps the
/// destination's registration.
async fn oauth_clients<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().oauth_clients()).await?;
    let mut written = 0;
    for row in &data.oauth_clients {
        let named_row = format!("oauth client {} ({})", row.client_id, row.name);
        if row.retired {
            report.drop_row("oauth_clients", named_row, "it was retired in v3");
            continue;
        }
        if present.contains(&row.client_id) {
            report.warn(format!(
                "{named_row}: this instance already registers the client id, so its own \
                 registration was kept"
            ));
            continue;
        }
        let app_data = app.data();
        Operations::new(app.gproxy(), &app_data, app.config())
            .oauth_clients()
            .create(OAuthClientWrite {
                id: row.client_id.clone(),
                name: row.name.clone(),
                redirect_uris: row.redirect_uris.clone(),
                enabled: Some(row.enabled),
            })
            .await
            .map_err(|error| Error::other(format!("{named_row}: {error}")))?;
        written += 1;
    }
    report.count("oauth_clients", written);
    Ok(())
}

/// The v4 operations a v3 operation group covered
/// (`v3:crates/gproxy-protocol/src/operation.rs`, `Operation::group`). Sora's
/// remix, edit, extend and character operations were in `video`; v4 has none
/// of them.
fn group_operations(group: &str) -> Option<&'static [&'static str]> {
    Some(match group {
        "models" => &["list_models", "get_model"],
        "count_tokens" => &["count_tokens"],
        "memories" => &["summarize_memory"],
        "generate_content" => &[
            "generate_content",
            "stream_generate_content",
            "guardian_review",
            "guardian_classify",
        ],
        "compact" => &["compact_content"],
        "conversation" => &["create_conversation"],
        "embeddings" => &["create_embedding", "batch_create_embedding"],
        "rerank" => &["rerank"],
        "search" => &["web_search"],
        "images" => &["create_image", "edit_image"],
        "audio" => &[
            "create_speech",
            "create_transcription",
            "create_translation",
        ],
        "files" => &[
            "create_file",
            "list_files",
            "retrieve_file",
            "retrieve_file_content",
            "delete_file",
        ],
        "video" => &[
            "create_video",
            "retrieve_video",
            "list_videos",
            "delete_video",
            "download_video_content",
        ],
        "realtime" => &["create_realtime_call", "connect_realtime"],
        _ => return None,
    })
}

async fn rate_limits<C>(
    app: &Arc<App<C>>,
    data: &document::Data,
    users: &HashSet<String>,
    keys: &HashSet<String>,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let present = existing(app.gproxy().store().rate_limits()).await?;
    let mut written = 0;
    for row in &data.rate_limits {
        let named_row = format!(
            "rate limit {} ({}:{})",
            row.id, row.subject_kind, row.subject_id
        );
        let Some(subject) = subject(&row.subject_kind, row.subject_id, users, keys) else {
            report.drop_row(
                "rate_limits",
                named_row,
                unmappable_subject(&row.subject_kind),
            );
            continue;
        };
        let id = ids::id("rate_limits", row.id);
        let write = RateLimitWrite {
            id: Some(id.clone()),
            user_id: subject.user_id.clone(),
            api_key_id: subject.api_key_id.clone(),
            // v3 counted requests and nothing else.
            metric: "requests".into(),
            limit_value: row.requests.to_string(),
            period_seconds: i64::try_from(row.window_seconds).unwrap_or(i64::MAX),
            model_pattern: None,
            enabled: Some(true),
        };
        let app_data = app.data();
        let operations = Operations::new(app.gproxy(), &app_data, app.config());
        if present.contains(&id) {
            operations
                .rate_limits()
                .update(
                    &id,
                    crate::dto::RateLimitPatch {
                        limit_value: Some(write.limit_value.clone()),
                        period_seconds: Some(write.period_seconds),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| named("rate limit", row.id, error))?;
        } else {
            operations
                .rate_limits()
                .create(write)
                .await
                .map_err(|error| named("rate limit", row.id, error))?;
        }
        written += 1;
    }
    report.count("rate_limits", written);
    Ok(())
}

/// v4's one-subject rule: a permission or a rate limit names a user or a key,
/// never both and never neither.
struct Subject {
    user_id: Option<String>,
    api_key_id: Option<String>,
}

fn subject(
    subject_kind: &str,
    subject_id: i64,
    users: &HashSet<String>,
    keys: &HashSet<String>,
) -> Option<Subject> {
    match subject_kind {
        "user" => {
            let id = ids::id("users", subject_id);
            users.contains(&id).then_some(Subject {
                user_id: Some(id),
                api_key_id: None,
            })
        }
        "user_key" => {
            let id = ids::id("user_keys", subject_id);
            keys.contains(&id).then_some(Subject {
                user_id: None,
                api_key_id: Some(id),
            })
        }
        _ => None,
    }
}

fn unmappable_subject(subject_kind: &str) -> String {
    match subject_kind {
        "organization" | "team" => format!(
            "v4 addresses a rule to one user or one key; a {subject_kind}-wide rule has no v4 \
             form and has to be written per member"
        ),
        other => format!(
            "its subject is `{other}`, which is neither a user nor a key that survived the import"
        ),
    }
}

// -------------------------------------------------------------- helpers --

/// Every id already in one identity table, so a re-run updates instead of
/// colliding.
async fn existing<C, E>(repository: gproxy_store::Repository<'_, C, E>) -> Result<HashSet<String>>
where
    C: BatchConnectionTrait,
    E: EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>,
    E::Model: IdColumn,
{
    Ok(repository
        .query(E::find())
        .await?
        .into_iter()
        .map(|row| row.row_id())
        .collect())
}

/// The primary key of a row, for the tables this module writes. A trait rather
/// than a closure per call so [`existing`] stays one function.
pub trait IdColumn {
    fn row_id(&self) -> String;
}

macro_rules! id_column {
    ($($model:path),+ $(,)?) => {$(
        impl IdColumn for $model {
            fn row_id(&self) -> String {
                self.id.clone()
            }
        }
    )+};
}

id_column!(
    organization::Model,
    team::Model,
    user::Model,
    api_key::Model,
    permission::Model,
    rate_limit::Model,
    oauth_client::Model,
);

/// An operation failure with the v3 row that caused it in front of it. Without
/// this the message is `name must not be blank` and an operator has a thousand
/// rows to look through.
fn named(entity: &'static str, id: i64, error: AppError) -> Error {
    Error::other(format!("v3 {entity} {id}: {error}"))
}

/// Resume identity import at a row boundary. Original rows remain available in
/// the archived tables; retries repeat at most one small, idempotent chunk.
pub(crate) async fn step<C>(
    app: &Arc<App<C>>,
    document: &Document,
    bridge: &Bridge,
    dropped: &std::collections::BTreeSet<i64>,
    cursor: usize,
) -> Result<(Report, Option<usize>)>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let sizes = [
        document.data.organizations.len(),
        document.data.teams.len(),
        document.data.users.len(),
        document.data.users.len(),
        document.data.user_keys.len(),
        document.data.permissions.len(),
        document.data.rate_limits.len(),
        document.data.oauth_clients.len(),
    ];
    let mut offset = cursor;
    let Some((group, size)) = sizes.into_iter().enumerate().find(|(_, n)| {
        if offset < *n {
            true
        } else {
            offset -= *n;
            false
        }
    }) else {
        return Ok((Report::default(), None));
    };
    let end = (offset + 16).min(size);
    let mut data = document.data.clone();
    let mut report = Report::default();
    let users = existing(app.gproxy().store().users()).await?;
    let keys = existing(app.gproxy().store().api_keys()).await?;
    match group {
        0 => {
            data.organizations = data.organizations[offset..end].to_vec();
            organizations(app, &data, &mut report).await?;
        }
        1 => {
            data.teams = data.teams[offset..end].to_vec();
            teams(app, &data, &mut report).await?;
        }
        2 => {
            data.users = data.users[offset..end].to_vec();
            self::users(app, &data, &mut report).await?;
        }
        3 => {
            data.users = data.users[offset..end].to_vec();
            memberships(app, &data, &users, &mut report).await?;
        }
        4 => {
            data.user_keys = data.user_keys[offset..end].to_vec();
            api_keys(app, &data, &users, bridge, &mut report).await?;
        }
        5 => {
            data.permissions = data.permissions[offset..end].to_vec();
            permissions(app, &data, &users, &keys, dropped, &mut report).await?;
        }
        6 => {
            data.rate_limits = data.rate_limits[offset..end].to_vec();
            rate_limits(app, &data, &users, &keys, &mut report).await?;
        }
        7 => {
            data.oauth_clients = data.oauth_clients[offset..end].to_vec();
            oauth_clients(app, &data, &mut report).await?;
        }
        _ => unreachable!(),
    }
    app.reload_all().await?;
    Ok((report, Some(cursor + end - offset)))
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_grouped_operation_is_a_v4_operation() {
        for group in [
            "models",
            "count_tokens",
            "memories",
            "generate_content",
            "compact",
            "conversation",
            "embeddings",
            "rerank",
            "search",
            "images",
            "audio",
            "files",
            "video",
            "realtime",
        ] {
            for operation in group_operations(group).expect(group) {
                assert!(
                    gproxy_sdk::Operation::from_id(operation).is_some(),
                    "{group}: {operation}"
                );
            }
        }
        assert!(group_operations("sora").is_none());
    }
    use super::*;

    #[test]
    fn a_retained_key_shows_its_body_and_an_unretained_one_shows_what_v3_knew() {
        assert_eq!(display_prefix("sk-abcdefghijkl"), "abcdefgh");
        assert_eq!(display_prefix("at-abcdefghijkl"), "abcdefgh");
        assert_eq!(display_prefix("short"), "short");
        assert_eq!(display_prefix("sk-"), UNKNOWN_PREFIX);
    }

    #[test]
    fn a_key_without_a_label_is_still_addressable_in_a_list() {
        let config: document::UserKeyConfig =
            serde_json::from_str(r#"{"id":7,"user_id":1,"label":"  "}"#).unwrap();
        assert_eq!(label(&config), "v3 key 7");
        let config: document::UserKeyConfig =
            serde_json::from_str(r#"{"id":7,"user_id":1,"label":"laptop"}"#).unwrap();
        assert_eq!(label(&config), "laptop");
    }

    #[test]
    fn only_a_user_or_a_key_is_a_v4_subject() {
        let users: HashSet<String> = ["v3-users-1".to_owned()].into();
        let keys: HashSet<String> = ["v3-user_keys-2".to_owned()].into();

        let user = subject("user", 1, &users, &keys).unwrap();
        assert_eq!(user.user_id.as_deref(), Some("v3-users-1"));
        assert!(user.api_key_id.is_none());

        let key = subject("user_key", 2, &users, &keys).unwrap();
        assert_eq!(key.api_key_id.as_deref(), Some("v3-user_keys-2"));
        assert!(key.user_id.is_none());

        // A scope-wide rule, and a subject whose row did not survive.
        assert!(subject("organization", 1, &users, &keys).is_none());
        assert!(subject("team", 1, &users, &keys).is_none());
        assert!(subject("user", 99, &users, &keys).is_none());
    }

    #[test]
    fn a_scope_wide_rule_is_explained_rather_than_dismissed() {
        for kind in ["organization", "team"] {
            let reason = unmappable_subject(kind);
            assert!(reason.contains("per member"), "{reason}");
        }
        assert!(unmappable_subject("surface").contains("neither a user nor a key"));
    }
}
