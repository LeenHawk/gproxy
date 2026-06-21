//! Provider-neutral domain records used by the [`PersistenceBackend`] trait.
//!
//! These are backend-agnostic shapes: the `db` impl maps them to/from SeaORM
//! models, the `file` impl serializes them as JSON. Domain code only ever sees
//! these types — never SeaORM entities.

pub mod audit;
pub mod authz;
pub mod identity;
pub mod logs;
pub mod provider;
pub mod routing;
pub mod settings;
pub mod transform;
pub mod usage;

pub use audit::{AuditLog, AuditLogInput};
pub use authz::{
    Quota, QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};
pub use identity::{Org, OrgInput, Team, TeamInput, User, UserInput, UserKey, UserKeyInput};
pub use logs::{DownstreamRequest, DownstreamRequestInput, UpstreamRequest, UpstreamRequestInput};
pub use provider::{
    Credential, CredentialInput, CredentialStatus, CredentialStatusInput, Provider, ProviderInput,
    ProviderModel, ProviderModelInput,
};
pub use routing::{Alias, AliasInput, Route, RouteInput, RouteMember, RouteMemberInput};
pub use settings::{InstanceSettings, InstanceSettingsInput};
pub use transform::{
    ProviderRuleSet, ProviderRuleSetInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet,
    RuleSetInput,
};
pub use usage::{Usage, UsageInput, UsageRollup, UsageRollupInput};
