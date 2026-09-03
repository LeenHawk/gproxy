use serde::{Deserialize, Serialize};

pub type Rest = serde_json::Map<String, serde_json::Value>;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Default,
    gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EmptyObject {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(untagged)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

extensible_string_enum!(ConversationRole, ConversationRoleKnown {
    User => "user", Assistant => "assistant", System => "system",
});
extensible_string_enum!(StopReason, StopReasonKnown {
    EndTurn => "end_turn", ToolUse => "tool_use", MaxTokens => "max_tokens",
    StopSequence => "stop_sequence", GuardrailIntervened => "guardrail_intervened",
    ContentFiltered => "content_filtered", MalformedModelOutput => "malformed_model_output",
    MalformedToolUse => "malformed_tool_use",
    ModelContextWindowExceeded => "model_context_window_exceeded",
});
extensible_string_enum!(ServiceTierType, ServiceTierTypeKnown {
    Priority => "priority", Default => "default", Flex => "flex", Reserved => "reserved",
});
extensible_string_enum!(PerformanceLatency, PerformanceLatencyKnown {
    Standard => "standard", Optimized => "optimized",
});
extensible_string_enum!(CachePointType, CachePointTypeKnown { Default => "default" });
extensible_string_enum!(CacheTtl, CacheTtlKnown { FiveMinutes => "5m", OneHour => "1h" });
extensible_string_enum!(ToolResultStatus, ToolResultStatusKnown {
    Success => "success", Error => "error",
});
extensible_string_enum!(ToolUseType, ToolUseTypeKnown {
    ServerToolUse => "server_tool_use",
});
extensible_string_enum!(GuardrailTrace, GuardrailTraceKnown {
    Enabled => "enabled", Disabled => "disabled", EnabledFull => "enabled_full",
});
extensible_string_enum!(GuardrailStreamProcessingMode, GuardrailStreamProcessingModeKnown {
    Sync => "sync", Async => "async",
});
extensible_string_enum!(GuardrailOrigin, GuardrailOriginKnown {
    Request => "REQUEST", AccountEnforced => "ACCOUNT_ENFORCED",
    OrganizationEnforced => "ORGANIZATION_ENFORCED",
});
extensible_string_enum!(GuardrailOwnership, GuardrailOwnershipKnown {
    SelfOwned => "SELF", CrossAccount => "CROSS_ACCOUNT",
});
extensible_string_enum!(GuardrailBlockAction, GuardrailBlockActionKnown {
    Blocked => "BLOCKED", None => "NONE",
});
extensible_string_enum!(GuardrailSensitiveAction, GuardrailSensitiveActionKnown {
    Anonymized => "ANONYMIZED", Blocked => "BLOCKED", None => "NONE",
});
extensible_string_enum!(GuardrailLevel, GuardrailLevelKnown {
    None => "NONE", Low => "LOW", Medium => "MEDIUM", High => "HIGH",
});
extensible_string_enum!(GuardrailContentFilterType, GuardrailContentFilterTypeKnown {
    Insults => "INSULTS", Hate => "HATE", Sexual => "SEXUAL", Violence => "VIOLENCE",
    Misconduct => "MISCONDUCT", PromptAttack => "PROMPT_ATTACK",
});
extensible_string_enum!(
    GuardrailContextualGroundingFilterType, GuardrailContextualGroundingFilterTypeKnown {
        Grounding => "GROUNDING", Relevance => "RELEVANCE",
    }
);
extensible_string_enum!(GuardrailTopicType, GuardrailTopicTypeKnown { Deny => "DENY" });
extensible_string_enum!(GuardrailManagedWordType, GuardrailManagedWordTypeKnown {
    Profanity => "PROFANITY",
});
extensible_string_enum!(
    GuardrailAutomatedReasoningLogicWarningType,
    GuardrailAutomatedReasoningLogicWarningTypeKnown {
        AlwaysFalse => "ALWAYS_FALSE", AlwaysTrue => "ALWAYS_TRUE",
    }
);
extensible_string_enum!(GuardrailPiiEntityType, GuardrailPiiEntityTypeKnown {
    Address => "ADDRESS", Age => "AGE", AwsAccessKey => "AWS_ACCESS_KEY",
    AwsSecretKey => "AWS_SECRET_KEY", CaHealthNumber => "CA_HEALTH_NUMBER",
    CaSocialInsuranceNumber => "CA_SOCIAL_INSURANCE_NUMBER",
    CreditDebitCardCvv => "CREDIT_DEBIT_CARD_CVV",
    CreditDebitCardExpiry => "CREDIT_DEBIT_CARD_EXPIRY",
    CreditDebitCardNumber => "CREDIT_DEBIT_CARD_NUMBER", DriverId => "DRIVER_ID",
    Email => "EMAIL", InternationalBankAccountNumber => "INTERNATIONAL_BANK_ACCOUNT_NUMBER",
    IpAddress => "IP_ADDRESS", LicensePlate => "LICENSE_PLATE", MacAddress => "MAC_ADDRESS",
    Name => "NAME", Password => "PASSWORD", Phone => "PHONE", Pin => "PIN",
    SwiftCode => "SWIFT_CODE", UkNationalHealthServiceNumber => "UK_NATIONAL_HEALTH_SERVICE_NUMBER",
    UkNationalInsuranceNumber => "UK_NATIONAL_INSURANCE_NUMBER",
    UkUniqueTaxpayerReferenceNumber => "UK_UNIQUE_TAXPAYER_REFERENCE_NUMBER", Url => "URL",
    Username => "USERNAME", UsBankAccountNumber => "US_BANK_ACCOUNT_NUMBER",
    UsBankRoutingNumber => "US_BANK_ROUTING_NUMBER",
    UsIndividualTaxIdentificationNumber => "US_INDIVIDUAL_TAX_IDENTIFICATION_NUMBER",
    UsPassportNumber => "US_PASSPORT_NUMBER", UsSocialSecurityNumber => "US_SOCIAL_SECURITY_NUMBER",
    VehicleIdentificationNumber => "VEHICLE_IDENTIFICATION_NUMBER",
});
extensible_string_enum!(OutputFormatType, OutputFormatTypeKnown {
    JsonSchema => "json_schema",
});
extensible_string_enum!(ImageFormat, ImageFormatKnown {
    Png => "png", Jpeg => "jpeg", Gif => "gif", Webp => "webp",
});
extensible_string_enum!(DocumentFormat, DocumentFormatKnown {
    Pdf => "pdf", Csv => "csv", Doc => "doc", Docx => "docx", Xls => "xls",
    Xlsx => "xlsx", Html => "html", Txt => "txt", Md => "md",
});
extensible_string_enum!(AsyncInvokeStatus, AsyncInvokeStatusKnown {
    InProgress => "InProgress", Completed => "Completed", Failed => "Failed",
});
extensible_string_enum!(CustomizationType, CustomizationTypeKnown {
    FineTuning => "FINE_TUNING", ContinuedPreTraining => "CONTINUED_PRE_TRAINING",
    Distillation => "DISTILLATION",
});
extensible_string_enum!(InferenceType, InferenceTypeKnown {
    OnDemand => "ON_DEMAND", Provisioned => "PROVISIONED",
});
extensible_string_enum!(ModelModality, ModelModalityKnown {
    Text => "TEXT", Image => "IMAGE", Embedding => "EMBEDDING",
});
extensible_string_enum!(FoundationModelLifecycleStatus, FoundationModelLifecycleStatusKnown {
    Active => "ACTIVE", Legacy => "LEGACY",
});
