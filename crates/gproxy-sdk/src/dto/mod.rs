//! Serializable shapes the management, login and query families exchange.
//!
//! Every id is a `String` and every timestamp is Unix milliseconds, so a DTO
//! reads the same from Rust, from the console and from a `ts-rs` declaration.
//! Decimal amounts travel as decimal strings for the same reason: a price or a
//! budget must survive a JavaScript `Number` unchanged.
//!
//! Three shapes per family, deliberately distinct. `…Dto` is what a read
//! returns; `…Write` is a whole new row, so its required fields are required;
//! `…Patch` changes some columns of an existing row, so every field is
//! optional. A nullable column's patch field is `Option<Option<T>>`: absent
//! leaves the column alone, `null` clears it.
//!
//! No DTO ever carries `credentials.secret`. A credential reports whether it
//! has one; reading it back is a separate, auditable operation.

mod common;
mod control;
mod endpoints;
mod login;
mod logs;
mod pricing;
mod profiles;
mod quota;
mod rewrite;
mod routing;
mod settings;
mod usage;

pub use common::{BatchItem, BatchPatch, ListQuery, Page, double_option};
pub use control::{
    CredentialDto, CredentialPatch, CredentialSummaryDto, CredentialWrite, ModelDto, ModelPatch,
    ModelWrite, ProviderDto, ProviderModelDto, ProviderModelPatch, ProviderModelWrite,
    ProviderPatch, ProviderWrite,
};
pub use endpoints::{
    OperationEndpointDto, OperationEndpointPatch, OperationEndpointWrite, OperationRuleDto,
    OperationRulePatch, OperationRuleWrite,
};
pub use login::{
    AuthCodeComplete, AuthCodeStart, AuthCodeStarted, CookieExchange, CredentialCreated,
    CredentialOwner, DevicePollOutcome, DeviceStart, DeviceStarted,
};
pub use logs::{
    CaptureEventDto, CaptureRecordDto, LogBodyDto, LogBodyEncoding, LogDetailDto, LogEntryDto,
    LogPageDto, LogQuery,
};
pub use pricing::{
    PriceRateDto, PriceRatePatch, PriceRateWrite, PriceRuleDto, PriceRulePatch, PriceRuleWrite,
    PriceTierDto, PriceTierPatch, PriceTierWrite,
};
pub use profiles::{ConnectionProfileDto, ConnectionProfilePatch, ConnectionProfileWrite};
pub use quota::{
    BudgetStatusDto, CountedWindowDto, CredentialBlockDto, CredentialCycleDto,
    CredentialLimitStatusDto, CredentialQuotaDto, QuotaAllowanceDto, QuotaBalanceDto, QuotaDto,
    QuotaEntryDto, QuotaPatch, QuotaResetDto, QuotaSettlementDto, QuotaSnapshotDto, QuotaWindowDto,
    QuotaWindowQuery, QuotaWrite,
};
pub use rewrite::{
    ProviderRuleSetDto, ProviderRuleSetPatch, ProviderRuleSetWrite, RewriteRuleDto,
    RewriteRulePatch, RewriteRuleWrite, RuleSetDto, RuleSetPatch, RuleSetWrite,
};
pub use routing::{
    ExposedModelDto, ExposedModelPatch, ExposedModelWrite, RouteDto, RouteMemberDto,
    RouteMemberPatch, RouteMemberWrite, RoutePatch, RouteWrite,
};
pub use settings::{
    InstanceSettingsDto, InstanceSettingsPatch, LoggingSettingsDto, LoggingSettingsPatch,
    SettingsDto, SettingsPatch,
};
pub(crate) use usage::money;
pub use usage::{
    UsageExchangeDto, UsageGroupBy, UsageGroupDto, UsageGroupQuery, UsageQuery, UsageRecordDto,
    UsageRecordQuery, UsageSummaryDto, UsageTokensDto, UsageTrendPointDto, UsageTrendQuery,
};
