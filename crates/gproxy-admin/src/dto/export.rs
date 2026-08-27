use std::path::PathBuf;

use ts_rs::{Config, TS};

use super::*;

#[test]
fn export_console_types() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../console/src/generated");
    if output.exists() {
        std::fs::remove_dir_all(&output).expect("clear generated TypeScript");
    }
    std::fs::create_dir_all(&output).expect("create generated TypeScript directory");
    let config = Config::new().with_out_dir(&output).with_large_int("number");
    macro_rules! export {
        ($($ty:ty),+ $(,)?) => {$({
            <$ty>::export_all(&config).expect(concat!("export ", stringify!($ty)));
        })+};
    }
    export!(
        crate::route::Entity,
        AdminIdentityDto,
        SessionStatusDto,
        SetupRequest,
        LoginRequest,
        AuthResponse,
        IdResponse,
        AppliedResponse,
        BatchActionDto,
        BatchRequest,
        BatchItemOutcome,
        BatchResponse,
        ErrorEnvelope,
        ChannelSupportDto,
        ChannelDto,
        ChannelFieldControlDto,
        ChannelFieldDto,
        LoginModeDto,
        LoginParamKindDto,
        LoginParamDto,
        ChannelLoginDto,
        AuthCodeStartRequest,
        AuthCodeStartResponse,
        AuthCodeCompleteRequest,
        DeviceStartRequest,
        DeviceStartResponse,
        DevicePollRequest,
        DevicePollResponse,
        CookieExchangeRequest,
        TlsPresetDto,
        AlpnDto,
        TlsVersionDto,
        PseudoHeaderDto,
        TlsProfileDto,
        Http2ProfileDto,
        TlsFingerprintDto,
        ProviderDto,
        ProviderWriteRequest,
        CredentialHealthDto,
        CredentialDto,
        CredentialWriteRequest,
        RouteDto,
        RouteWriteRequest,
        RouteMemberDto,
        RouteMemberWriteRequest,
        AliasDto,
        AliasWriteRequest,
        ModelAliasDto,
        ModelAliasWriteRequest,
        RoutingImplementationDto,
        RoutingRuleDto,
        RoutingRuleWriteRequest,
        RuleSetDto,
        RuleSetWriteRequest,
        TextPositionDto,
        RewriteActionDto,
        HeaderModeDto,
        TransformPhaseDto,
        TransformLocateDto,
        TransformActionDto,
        RuleConfigDto,
        RuleDto,
        RuleWriteRequest,
        ProviderRuleSetDto,
        ProviderRuleSetWriteRequest,
        PriceRuleDto,
        PriceRuleWriteRequest,
        PriceRateDto,
        PriceRateWriteRequest,
        OrganizationDto,
        OrganizationWriteRequest,
        TeamDto,
        TeamWriteRequest,
        UserDto,
        UserWriteRequest,
        UserKeyPrefix,
        UserKeyDto,
        UserKeyCreateRequest,
        UserKeyUpdateRequest,
        UserKeyCreateResponse,
        UserKeyRevealResponse,
        PermissionDto,
        PermissionWriteRequest,
        RateLimitDto,
        RateLimitWriteRequest,
        QuotaDto,
        QuotaWriteRequest,
        UsageGroupByDto,
        UsageQueryDto,
        UsageAggregateDto,
        QuotaWindowDto,
        BoundarySourceDto,
        BoundaryConfidenceDto,
        QuotaCoverageDto,
        QuotaCycleStatusDto,
        QuotaCycleCloseReasonDto,
        CredentialQuotaCycleDto,
        LogQueryDto,
        LogListItemDto,
        LogPageDto,
        DownstreamLogDto,
        WireLogDto,
        LogDetailDto,
        LogSettingsDto,
        LogSettingsUpdateDto,
        InstanceSettingsDto,
        TokenizerVocabDto,
        TokenizerFetchRequest,
        AuditEventDto,
        PortalContextDto,
        PortalSettingsDto,
        PortalModelCapabilityDto,
        PortalModelDto,
        PortalUsageQueryDto,
        PortalUsageDto,
        PortalQuotaScopeDto,
        PortalQuotaWindowKindDto,
        PortalQuotaWindowDto,
        PortalRecentQueryDto,
        PortalRecentRequestDto,
    );
    let mut names = std::fs::read_dir(&output)
        .expect("read generated TypeScript")
        .map(|entry| entry.expect("generated entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter_map(|name| name.strip_suffix(".ts").map(str::to_owned))
        .filter(|name| name != "index")
        .collect::<Vec<_>>();
    names.sort();
    let index = names
        .into_iter()
        .map(|name| format!("export * from \"./{name}\";\n"))
        .collect::<String>();
    std::fs::write(output.join("index.ts"), index).expect("write generated TypeScript index");
}
