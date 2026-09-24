//! The wire shapes of the identity management API and of the self-serve
//! portal.
//!
//! Conventions are the sdk's, deliberately to the letter: every id is a
//! `String`, every timestamp is Unix milliseconds, every field is camelCase on
//! the wire, and decimal amounts travel as decimal strings so a budget or a
//! limit survives a JavaScript `Number` unchanged. The v3 numeric DTOs cannot
//! be carried over — v4 entities have string primary keys.
//!
//! Three shapes per family. `…Dto` is what a read returns; `…Write` is a whole
//! new row, so its required fields are required; `…Patch` changes some columns
//! of an existing one, so every field is optional and a nullable column's
//! field is `Option<Option<T>>` — absent leaves the column alone, `null`
//! clears it.
//!
//! **What never appears in any of these.** `users.password_hash`,
//! `api_keys.key_hash`, the bytes of `api_keys.secret` and
//! `user_sessions.token_hash` have no DTO field and no accessor. A user
//! reports `hasPassword`, a key reports its display `prefix` and `hasSecret`,
//! and a session reports only when it was opened and when it ends. The one
//! place a plaintext key exists is [`ApiKeyCreated::token`], returned by the
//! mint and the rotation and by nothing else, plus the explicit
//! [`reveal`](crate::operations::ApiKeys::reveal) of a key whose secret the
//! instance was asked to retain.
//!
//! **The portal shapes have one more rule.** They belong to the end user's
//! surface, so none of them names anybody but the caller: no `userId` on a
//! key, none on a request row, and no request field through which a caller
//! could ask about somebody else. The single exception,
//! [`PortalUsageQuery::user_id`], exists so a console can post one filter
//! object to both surfaces, and is overwritten with the caller's own id before
//! the query runs rather than read.

mod admin_context;
mod api_keys;
mod audit;
mod common;
mod credential_management;
mod members;
mod oauth;
mod oauth_clients;
mod organizations;
mod permissions;
mod portal;
mod rate_limits;
mod sessions;
mod teams;
mod users;

/// The `ts-rs` declarations, and the check that the export list still covers
/// every `pub use` below. Tests only, and only where a filesystem exists.
#[cfg(all(test, feature = "ts", not(target_arch = "wasm32")))]
mod export;

pub use admin_context::{AdminContextDto, AdminContextUserDto, AdminScopeDto, AdminSectionDto};
pub use api_keys::{
    ApiKeyBudgetWrite, ApiKeyCreated, ApiKeyDto, ApiKeyPatch, ApiKeySecretDto, ApiKeyWrite,
};
pub use audit::{AuditEventDto, AuditQuery};
pub use common::{BatchItem, BatchPatch, ListQuery, Page, double_option};
pub use credential_management::{CredentialOwnerOptionDto, CredentialProviderDto};
pub use members::{MemberPatch, MemberWrite, MembershipDto, OrganizationMemberDto, TeamMemberDto};
pub use oauth::{
    AuthorizationDenied, AuthorizationIssued, AuthorizationServerMetadata, AuthorizeDetails,
    AuthorizeOutcome, AuthorizeQuery, ConsentDecision, DEVICE_GRANT_TYPE, DeviceCodeRequest,
    DeviceCodeResponse, DeviceDecided, DeviceDetails, OAuthErrorBody, RevokeRequest, TOKEN_TYPE,
    TokenRequest, TokenResponse,
};
pub use oauth_clients::{OAuthClientDto, OAuthClientPatch, OAuthClientWrite};
pub use organizations::{OrganizationDto, OrganizationPatch, OrganizationWrite};
pub use permissions::{PermissionDto, PermissionPatch, PermissionWrite};
pub use portal::{
    PortalContextDto, PortalFeaturesDto, PortalKeyCreate, PortalKeyCreated, PortalKeyDto,
    PortalKeySecretDto, PortalLogin, PortalModelDto, PortalOAuthSessionDto, PortalOrganizationDto,
    PortalPasswordChange, PortalQuotaWindowDto, PortalRequestDto, PortalSessionDto, PortalTeamDto,
    PortalUsageDto, PortalUsageQuery, PortalUserDto,
};
pub use rate_limits::{RateLimitDto, RateLimitPatch, RateLimitWrite};
pub use sessions::UserSessionDto;
pub use teams::{TeamDto, TeamPatch, TeamWrite};
pub use users::{PasswordWrite, UserDto, UserPatch, UserWrite};

/// A stored JSON allowlist column as a DTO field.
///
/// `None` and a value that is not an array of strings both read as "inherits";
/// a malformed row must not make a list request fail, for the same reason
/// snapshot assembly drops a malformed row rather than refusing to publish.
pub(crate) fn allowlist(value: Option<serde_json::Value>) -> Option<Vec<String>> {
    let entries = value?;
    let entries = entries.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_owned))
            .collect(),
    )
}
