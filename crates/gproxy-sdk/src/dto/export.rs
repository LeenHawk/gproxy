//! TypeScript declarations for everything `dto` exchanges.
//!
//! The list below is hand-written, because Rust offers no way to enumerate a
//! module's types at run time. A hand-written list is exactly what drifted in
//! v3 — a DTO was added, nobody remembered the list, and the console silently
//! went without a type for it — so the second test here reads `mod.rs` itself
//! and fails when the two disagree. Adding a `pub use` to `dto` and forgetting
//! this file is a red test, not a missing file.
//!
//! Exporting writes to a directory the caller names in `GPROXY_TS_OUT`. Without
//! that variable the test returns immediately and writes nothing, so
//! `cargo test --all-features` stays hermetic and the console's generated
//! directory is only ever rewritten on purpose.

use std::{collections::BTreeSet, path::PathBuf};

use ts_rs::{Config, Dummy, TS};

#[allow(unused_imports)]
use crate::dto::*;

/// One list, two readers: the exporter walks it, and the coverage test
/// compares the names in it against the `pub use` items of `mod.rs`.
macro_rules! exported {
    ($($ty:ty),+ $(,)?) => {
        fn export_every_type(config: &Config) {
            $(
                <$ty>::export_all(config).unwrap_or_else(|error| {
                    panic!("export {}: {error}", stringify!($ty))
                });
            )+
        }

        const EXPORTED: &[&str] = &[$(stringify!($ty)),+];
    };
}

exported!(
    // the channel catalogue, defined in `gproxy-channel`
    ChannelCapabilities,
    ChannelDescriptor,
    ConfigKey,
    ConfigKeyKind,
    LoginMode,
    // catalog
    ApplyDefaultPricesReportDto,
    ApplyDefaultPricesRequest,
    ApplyRulePreset,
    DefaultModelCatalogDto,
    DefaultModelCatalogSourceDto,
    DefaultModelDto,
    DefaultModelPriceRateDto,
    DefaultModelPricingDto,
    DefaultModelTierDto,
    RulePresetCategory,
    RulePresetDto,
    TlsPresetDto,
    // common. The generic parameters are placeholders: `ts-rs` declares the
    // generic type and never mentions the argument it was named with.
    BatchItem<Dummy, Dummy>,
    BatchPatch<Dummy>,
    ListQuery,
    Page<Dummy>,
    // connectivity
    ConnectivityResultDto,
    ConnectivityScope,
    ConnectivityTest,
    DiscoveredModelDto,
    ModelTest,
    ModelTestResultDto,
    // control
    CredentialDto,
    CredentialPatch,
    CredentialSummaryDto,
    CredentialWrite,
    ModelDto,
    ModelPatch,
    ModelWrite,
    ProviderDto,
    ProviderModelDto,
    ProviderModelPatch,
    ProviderModelWrite,
    ProviderPatch,
    ProviderWrite,
    // endpoints
    OperationEndpointDto,
    OperationEndpointPatch,
    OperationEndpointWrite,
    OperationRuleDto,
    OperationRulePatch,
    OperationRuleWrite,
    // login
    AuthCodeComplete,
    AuthCodeStart,
    AuthCodeStarted,
    CookieExchange,
    CredentialCreated,
    CredentialOwner,
    DevicePollOutcome,
    DeviceStart,
    DeviceStarted,
    // logs
    CaptureEventDto,
    CaptureRecordDto,
    LogBodyDto,
    LogBodyEncoding,
    LogDetailDto,
    LogEntryDto,
    LogPageDto,
    LogQuery,
    // pricing
    PriceRateDto,
    PriceRatePatch,
    PriceRateWrite,
    PriceRuleDto,
    PriceRulePatch,
    PriceRuleWrite,
    PriceTierDto,
    PriceTierPatch,
    PriceTierWrite,
    // profiles
    ConnectionProfileDto,
    ConnectionProfilePatch,
    ConnectionProfileWrite,
    // quota
    BudgetStatusDto,
    CountedWindowDto,
    CredentialBlockDto,
    CredentialCycleDto,
    CredentialLimitStatusDto,
    CredentialQuotaDto,
    QuotaAllowanceDto,
    QuotaBalanceDto,
    QuotaDto,
    QuotaEntryDto,
    QuotaPatch,
    QuotaResetDto,
    QuotaSettlementDto,
    QuotaSnapshotDto,
    QuotaWindowDto,
    QuotaWindowQuery,
    QuotaWrite,
    // rewrite
    ProviderRuleSetDto,
    ProviderRuleSetPatch,
    ProviderRuleSetWrite,
    RewriteRuleDto,
    RewriteRulePatch,
    RewriteRuleWrite,
    RuleSetDto,
    RuleSetPatch,
    RuleSetWrite,
    // routing
    ExposedModelDto,
    ExposedModelPatch,
    ExposedModelWrite,
    RouteDto,
    RouteMemberDto,
    RouteMemberPatch,
    RouteMemberWrite,
    RoutePatch,
    RouteWrite,
    // settings
    InstanceSettingsDto,
    InstanceSettingsPatch,
    LoggingSettingsDto,
    LoggingSettingsPatch,
    SettingsDto,
    SettingsPatch,
    // transfer
    ConfigurationDataDto,
    ConfigurationExportDto,
    ExportCredentialDto,
    ExportRequest,
    ImportMode,
    ImportReportDto,
    ImportRequest,
    SealedSecretDto,
    // usage
    UsageExchangeDto,
    UsageGroupBy,
    UsageGroupDto,
    UsageGroupQuery,
    UsageQuery,
    UsageRecordDto,
    UsageRecordQuery,
    UsageSummaryDto,
    UsageTokensDto,
    UsageTrendPointDto,
    UsageTrendQuery,
    // tokenizer, declared in `manage` and re-exported by `dto`
    TokenizerAuthDto,
    TokenizerFetch,
    TokenizerProgressDto,
    VocabularyDto,
);

/// Write every declaration into `GPROXY_TS_OUT`, or do nothing at all.
///
/// The directory is wiped first: a stale file for a DTO that no longer
/// exists would keep compiling in the console long after the Rust side
/// dropped it. `with_large_int("number")` is what makes a `i64`
/// millisecond timestamp a `number` rather than a `bigint` — every
/// timestamp and byte count in these DTOs is far inside the range a
/// JavaScript number holds exactly, and the amounts that are not are
/// already decimal strings.
#[test]
fn export_types() {
    let Ok(out) = std::env::var("GPROXY_TS_OUT") else {
        return;
    };
    let out = PathBuf::from(out);
    if out.exists() {
        std::fs::remove_dir_all(&out).expect("clear the output directory");
    }
    std::fs::create_dir_all(&out).expect("create the output directory");

    let config = Config::new().with_out_dir(&out).with_large_int("number");
    export_every_type(&config);

    let mut names = std::fs::read_dir(&out)
        .expect("read the output directory")
        .map(|entry| entry.expect("read an output entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter_map(|name| name.strip_suffix(".ts").map(str::to_owned))
        .filter(|name| name != "index")
        .collect::<Vec<_>>();
    names.sort();
    let index = names
        .into_iter()
        .map(|name| format!("export * from \"./{name}\";\n"))
        .collect::<String>();
    std::fs::write(out.join("index.ts"), index).expect("write the index");
}

/// The list above against the `pub use` items of `mod.rs`.
///
/// Reading the source is the point. Comparing two hand-written lists would
/// only move the drift one file over; reading the module means the check
/// fails the moment a DTO is re-exported without being added here.
#[test]
fn every_exported_dto_is_in_the_export_list() {
    let declared = public_type_names(include_str!("mod.rs"));
    let exported = EXPORTED
        .iter()
        .map(|spec| simple_name(spec).to_owned())
        .collect::<BTreeSet<_>>();

    let missing = declared.difference(&exported).collect::<Vec<_>>();
    let unknown = exported.difference(&declared).collect::<Vec<_>>();
    assert!(
        missing.is_empty() && unknown.is_empty(),
        "dto/export.rs is out of step with dto/mod.rs\n  \
         re-exported but never exported: {missing:?}\n  \
         exported but not re-exported by `dto`: {unknown:?}"
    );
}

/// Every type named by a `pub use` statement of the given module source.
///
/// A statement may span several lines, so this tracks the `;`. Only the
/// crate's own `pub use` counts — `pub(crate) use` is an internal helper,
/// and this crate has one (`money`).
fn public_type_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut inside = false;
    for line in source.lines().map(str::trim) {
        if line.starts_with("//") {
            continue;
        }
        if !inside && !line.starts_with("pub use ") {
            continue;
        }
        for token in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if is_type_name(token) {
                names.insert(token.to_owned());
            }
        }
        inside = !line.ends_with(';');
    }
    names
}

/// Whether an item name is a type. Modules and free functions are
/// lowercase, constants are `SCREAMING_SNAKE_CASE`; only a type is
/// `UpperCamelCase`.
fn is_type_name(token: &str) -> bool {
    token.starts_with(char::is_uppercase) && token.chars().any(char::is_lowercase)
}

/// `Page < Dummy >` as `stringify!` spells it, back to `Page`.
fn simple_name(spec: &str) -> &str {
    let head = spec.split('<').next().unwrap_or(spec).trim();
    head.rsplit("::").next().unwrap_or(head).trim()
}
