//! The wire shapes of the self-serve portal.
//!
//! Same conventions as the rest of [`crate::dto`] — String ids, Unix
//! milliseconds, camelCase on the wire, decimal amounts as strings — with one
//! rule of its own that the admin shapes do not have:
//!
//! **no portal shape names anybody but the caller.** There is no `userId` on a
//! key, no `userId` on a request row, no owner on a quota window that is not
//! the caller's own chain, and no request field in which a caller could ask
//! about somebody else. The scoping of
//! [`Portal`](crate::operations::Portal) is enforced in the operations; these
//! types are the second half of it, because a field that cannot be spelled
//! cannot be trusted by mistake.
//!
//! The one deliberate exception is [`PortalUsageQuery::user_id`], which exists
//! so a console may post the same filter object it uses on the admin side. It
//! is **overwritten** with the caller's own id before the query runs, never
//! read.

use gproxy_sdk::dto::{
    LogEntryDto, UsageGroupBy, UsageGroupDto, UsageSummaryDto, UsageTrendPointDto,
};
use gproxy_store::entity::{identity::api_key, oauth::grant};
use serde::{Deserialize, Serialize};

use super::ApiKeyDto;

// ------------------------------------------------------------- context ----

/// Everything a portal needs to render its shell, in one call: who you are,
/// which scopes you belong to, what you are subscribed to, and which parts of
/// the portal this instance offers you.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalContextDto {
    pub user: PortalUserDto,
    /// Direct organization memberships. A team joined without its parent is
    /// *not* listed here; the team is, with its `organizationId`.
    pub organizations: Vec<PortalOrganizationDto>,
    pub teams: Vec<PortalTeamDto>,
    pub features: PortalFeaturesDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalUserDto {
    pub id: String,
    pub name: String,
    /// The instance-wide role, `admin` or `user`. Organization and team roles
    /// are memberships and are listed with their scope.
    pub role: String,
    /// Whether a password is set. An account without one signs in through
    /// OAuth only, and its password change takes no `current`.
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalOrganizationDto {
    pub id: String,
    pub name: String,
    /// `member` or `admin`, scoped to this organization.
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalTeamDto {
    pub id: String,
    pub name: String,
    pub organization_id: String,
    /// `member` or `admin`, scoped to this team. An organization admin is not
    /// automatically a team admin.
    pub role: String,
}

/// What this instance lets this caller do, so a portal can hide a tab rather
/// than render one that answers 403.
///
/// Every flag is a fact about configuration or about the caller's role — never
/// about data — so computing it costs nothing and leaks nothing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalFeaturesDto {
    /// Whether the key family's writing half is open. False for an OAuth-grant
    /// caller: a token the user handed to somebody else's program must not be
    /// able to mint a fresh long-lived credential out of that trust.
    pub can_create_keys: bool,
    /// Same rule, same reason: a third-party program does not get to change
    /// the account password it was lent.
    pub can_change_password: bool,
    /// `settings.portal_recent_requests_enabled`. Off means
    /// [`Portal::recent_requests`](crate::operations::Portal::recent_requests)
    /// answers with nothing.
    pub can_see_logs: bool,
    /// Whether to offer a link into the operator console: it has to be built
    /// into this instance *and* the caller has to be an instance administrator.
    pub can_see_console: bool,
}

// -------------------------------------------------------------- models ----

/// One addressable model name, and whether this caller may call it.
///
/// `providerCount` and `channelIds` describe the providers that back the name
/// on this instance, not the ones this caller may reach — a count and a
/// channel id are instance configuration, and knowing that `codex/gpt-5` is
/// served by two providers tells a user nothing about another tenant.
/// `permitted` is the caller's own answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalModelDto {
    /// Either an exposed model name or a `providerName/model` form.
    pub name: String,
    pub provider_count: u64,
    /// The channels behind the name, sorted and deduplicated.
    pub channel_ids: Vec<String>,
    /// Whether at least one of those providers is one the caller's permission
    /// rules allow for this name.
    pub permitted: bool,
}

// ---------------------------------------------------------------- keys ----

/// One of the caller's own gateway keys.
///
/// No `userId`: it is always the caller. No `kind`: the portal only ever shows
/// `user` keys, because an `oauth` key belongs to a grant and is managed by
/// revoking that grant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalKeyDto {
    pub id: String,
    pub name: String,
    /// The first characters of the key body, for telling two keys apart.
    pub prefix: String,
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub enabled: bool,
    /// Whether the instance retained a sealed copy, i.e. whether `reveal` can
    /// answer for this key.
    pub has_secret: bool,
}

impl From<ApiKeyDto> for PortalKeyDto {
    fn from(key: ApiKeyDto) -> Self {
        Self {
            id: key.id,
            name: key.name,
            prefix: key.prefix,
            organization_id: key.organization_id,
            team_id: key.team_id,
            expires_at_ms: key.expires_at_ms,
            enabled: key.enabled,
            has_secret: key.has_secret,
        }
    }
}

impl From<api_key::Model> for PortalKeyDto {
    fn from(row: api_key::Model) -> Self {
        ApiKeyDto::from(row).into()
    }
}

/// What a portal user may choose when minting a key.
///
/// There is no `userId`: the key is the caller's. The two binding fields are
/// present because a user who belongs to a team wants a key scoped to it, and
/// they are validated by the same rules the admin family applies — the
/// organization and the team must exist, and **the caller must be a member of
/// them**.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalKeyCreate {
    pub name: String,
    /// Absent mints a key that never expires.
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    /// Keep a sealed copy so the key can be revealed later. False by default:
    /// a key the instance cannot reproduce is a key a database leak does not
    /// hand over.
    #[serde(default)]
    pub retain_secret: Option<bool>,
}

/// A minted or rotated key. `token` is the only time the plaintext travels,
/// apart from an explicit reveal of a key that retained one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalKeyCreated {
    #[serde(flatten)]
    pub key: PortalKeyDto,
    pub token: String,
}

/// The answer to a reveal. Separate from [`PortalKeyDto`] so a plaintext can
/// never ride along with an ordinary read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalKeySecretDto {
    pub id: String,
    pub token: String,
}

// --------------------------------------------------------------- usage ----

/// What a portal asks about its own usage.
///
/// `userId` is accepted and **ignored**: a console shares one filter object
/// with the admin surface, and refusing the field would only push the caller
/// into sending the same request without it. It is overwritten with the
/// caller's own id before the query reaches the engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalUsageQuery {
    /// Inclusive lower bound on `startedAtMs`.
    #[serde(default)]
    pub from_ms: Option<i64>,
    /// Exclusive upper bound.
    #[serde(default)]
    pub to_ms: Option<i64>,
    /// Ignored. See the type note.
    #[serde(default)]
    pub user_id: Option<String>,
    /// One of the caller's own keys. A key that is not theirs matches nothing,
    /// because the user filter is applied as well.
    #[serde(default)]
    pub api_key_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    /// Admin surface only: the share of one upstream provider's attempts.
    /// A portal refuses it, because it never shows a caller provider ids.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Admin surface only: the share of one credential's attempts. A portal
    /// refuses it for the same reason it refuses `groupBy: credential` —
    /// credential ids are the operator's, not the caller's.
    #[serde(default)]
    pub credential_id: Option<String>,
    /// Absent means no grouped cut is computed.
    #[serde(default)]
    pub group_by: Option<UsageGroupBy>,
    /// Bucket width for the trend. Absent means no trend is computed; present
    /// requires both `fromMs` and `toMs`.
    #[serde(default)]
    pub bucket_ms: Option<i64>,
}

/// The caller's own usage over the requested range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalUsageDto {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub summary: UsageSummaryDto,
    /// Empty unless `groupBy` was given.
    pub groups: Vec<UsageGroupDto>,
    /// Empty unless `bucketMs` was given.
    pub trend: Vec<UsageTrendPointDto>,
}

// --------------------------------------------------------------- quota ----

/// One budget window of the caller's own chain.
///
/// The owner is reported because a user has to be able to tell "I am out of my
/// own allowance" from "my team is out of its allowance", which are two very
/// different things to do something about. Every owner here is one the caller
/// is themselves part of: the chain is built from their own key binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalQuotaWindowDto {
    /// `api_key`, `user`, `team` or `org`.
    pub owner_kind: String,
    pub owner_id: String,
    /// The budget's stable key within its owner, e.g. `primary`.
    pub window_key: String,
    /// `5h`, `1d`, `7d`, `1m`, `total`, …
    pub period: String,
    /// The glob this budget covers, absent when it covers everything.
    pub model_pattern: Option<String>,
    /// `USD` for a cost budget.
    pub unit: String,
    pub used: String,
    pub limit: String,
    /// On the 0..100 scale, to two decimals. None when the limit is zero,
    /// which is a switched-off subject rather than a ratio.
    pub used_percent: Option<String>,
    pub starts_at_ms: i64,
    /// None for a permanent (`total`) window.
    pub resets_at_ms: Option<i64>,
}

// ----------------------------------------------------------- requests ----

/// One of the caller's own recent requests, reduced.
///
/// What is missing is the point: no bodies, no headers, no URL, no client
/// address, no credential id, and no provider id — only the provider's display
/// name, and only when it still exists. See the module note on
/// [`Portal::recent_requests`](crate::operations::Portal::recent_requests).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalRequestDto {
    pub request_id: String,
    /// The caller's own key that made the request, when one did.
    pub api_key_id: Option<String>,
    /// The name the client asked for.
    pub model: Option<String>,
    pub operation: Option<String>,
    /// The upstream's display name. Never its id, never its credential.
    pub provider: Option<String>,
    pub response_status: Option<i32>,
    /// `in_progress`, `completed`, `failed` or `cancelled`. Capture
    /// completeness, not the upstream's verdict.
    pub state: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    /// End minus start, when the request has ended.
    pub duration_ms: Option<i64>,
}

impl PortalRequestDto {
    /// One admin log row, reduced. Taking [`LogEntryDto`] rather than the
    /// database row is the point: the reduction is visible as the fields that
    /// are dropped — `kind`, `sessionId`, `userId`, `credentialId`,
    /// `requestMethod`, `requestUrl`, `error`, `clientIp` — rather than hidden
    /// in a second query that happens to select less.
    ///
    /// `provider` is resolved by the caller of this function, which is the
    /// only thing that knows the live provider names.
    pub(crate) fn new(row: LogEntryDto, provider: Option<String>) -> Self {
        Self {
            duration_ms: row
                .ended_at_ms
                .map(|ended| ended.saturating_sub(row.started_at_ms)),
            request_id: row.request_id,
            api_key_id: row.api_key_id,
            model: row.model,
            operation: row.operation,
            provider,
            response_status: row.response_status,
            state: row.state,
            started_at_ms: row.started_at_ms,
            ended_at_ms: row.ended_at_ms,
        }
    }
}

// ------------------------------------------------------ oauth sessions ----

/// One live authorization the caller granted to a third-party client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalOAuthSessionDto {
    pub id: String,
    pub client_id: String,
    /// The client's registered display name, or None when the registration is
    /// gone.
    pub client_name: Option<String>,
    /// The scopes this grant carries, as granted.
    pub scopes: Vec<String>,
    pub created_at_ms: i64,
    /// The first successful token exchange. None means consent was given but
    /// the client never completed the login.
    pub logged_in_at_ms: Option<i64>,
    pub last_refreshed_at_ms: Option<i64>,
    pub refresh_count: i64,
    pub refresh_expires_at_ms: Option<i64>,
}

impl PortalOAuthSessionDto {
    pub(crate) fn new(row: grant::Model, client_name: Option<String>) -> Self {
        Self {
            id: row.id,
            client_id: row.client_id,
            client_name,
            scopes: scopes(&row.scopes),
            created_at_ms: row.created_at_ms,
            logged_in_at_ms: row.logged_in_at_ms,
            last_refreshed_at_ms: row.last_refreshed_at_ms,
            refresh_count: row.refresh_count,
            refresh_expires_at_ms: row.refresh_expires_at_ms,
        }
    }
}

/// `oauth_grants.scopes` as a list. A malformed column reads as no scopes
/// rather than failing the listing, for the same reason snapshot assembly
/// drops a malformed row instead of refusing to publish.
fn scopes(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------ password ----

/// A portal password change.
///
/// `current` is required when the account has a password and refused when it
/// does not: an OAuth-only account has nothing to prove, and asking it for a
/// password it never set would be unanswerable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalPasswordChange {
    #[serde(default)]
    pub current: Option<String>,
    pub new: String,
}

/// A portal sign-in.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalLogin {
    pub name: String,
    pub password: String,
}

/// The session a sign-in opened. `token` is the only copy; the row holds its
/// digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PortalSessionDto {
    pub id: String,
    pub token: String,
    pub expires_at_ms: i64,
}

impl From<crate::auth::IssuedSession> for PortalSessionDto {
    fn from(session: crate::auth::IssuedSession) -> Self {
        Self {
            id: session.id,
            token: session.token,
            expires_at_ms: session.expires_at_ms,
        }
    }
}
