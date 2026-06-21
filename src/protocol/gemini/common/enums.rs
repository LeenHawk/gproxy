use serde::{Deserialize, Serialize};

macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $first_variant:ident => $first_wire:literal $(, $variant:ident => $wire:literal)* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            #[serde(rename = $first_wire)]
            $first_variant,
            $(
                #[serde(rename = $wire)]
                $variant,
            )*
        }

        impl Default for $outer {
            fn default() -> Self {
                Self::Known($known::$first_variant)
            }
        }
    };
}

extensible_string_enum!(ContentRole, ContentRoleKnown {
    User => "user",
    Model => "model",
    System => "system",
    Function => "function",
});

extensible_string_enum!(Modality, ModalityKnown {
    ModalityUnspecified => "MODALITY_UNSPECIFIED",
    Text => "TEXT",
    Image => "IMAGE",
    Video => "VIDEO",
    Audio => "AUDIO",
    Document => "DOCUMENT",
});

extensible_string_enum!(ResponseModality, ResponseModalityKnown {
    ModalityUnspecified => "MODALITY_UNSPECIFIED",
    Text => "TEXT",
    Image => "IMAGE",
    Audio => "AUDIO",
});

extensible_string_enum!(ResponseMimeType, ResponseMimeTypeKnown {
    TextPlain => "text/plain",
    ApplicationJson => "application/json",
    TextXEnum => "text/x.enum",
});

extensible_string_enum!(HarmCategory, HarmCategoryKnown {
    HarmCategoryUnspecified => "HARM_CATEGORY_UNSPECIFIED",
    HarmCategoryDerogatory => "HARM_CATEGORY_DEROGATORY",
    HarmCategoryToxicity => "HARM_CATEGORY_TOXICITY",
    HarmCategoryViolence => "HARM_CATEGORY_VIOLENCE",
    HarmCategorySexual => "HARM_CATEGORY_SEXUAL",
    HarmCategoryMedical => "HARM_CATEGORY_MEDICAL",
    HarmCategoryDangerous => "HARM_CATEGORY_DANGEROUS",
    HarmCategoryHarassment => "HARM_CATEGORY_HARASSMENT",
    HarmCategoryHateSpeech => "HARM_CATEGORY_HATE_SPEECH",
    HarmCategorySexuallyExplicit => "HARM_CATEGORY_SEXUALLY_EXPLICIT",
    HarmCategoryDangerousContent => "HARM_CATEGORY_DANGEROUS_CONTENT",
    HarmCategoryCivicIntegrity => "HARM_CATEGORY_CIVIC_INTEGRITY",
});

extensible_string_enum!(HarmBlockThreshold, HarmBlockThresholdKnown {
    HarmBlockThresholdUnspecified => "HARM_BLOCK_THRESHOLD_UNSPECIFIED",
    BlockLowAndAbove => "BLOCK_LOW_AND_ABOVE",
    BlockMediumAndAbove => "BLOCK_MEDIUM_AND_ABOVE",
    BlockOnlyHigh => "BLOCK_ONLY_HIGH",
    BlockNone => "BLOCK_NONE",
    Off => "OFF",
});

extensible_string_enum!(HarmProbability, HarmProbabilityKnown {
    HarmProbabilityUnspecified => "HARM_PROBABILITY_UNSPECIFIED",
    Negligible => "NEGLIGIBLE",
    Low => "LOW",
    Medium => "MEDIUM",
    High => "HIGH",
});

extensible_string_enum!(ServiceTier, ServiceTierKnown {
    Unspecified => "unspecified",
    Standard => "standard",
    Flex => "flex",
    Priority => "priority",
});

extensible_string_enum!(FinishReason, FinishReasonKnown {
    FinishReasonUnspecified => "FINISH_REASON_UNSPECIFIED",
    Stop => "STOP",
    MaxTokens => "MAX_TOKENS",
    Safety => "SAFETY",
    Recitation => "RECITATION",
    Language => "LANGUAGE",
    Other => "OTHER",
    Blocklist => "BLOCKLIST",
    ProhibitedContent => "PROHIBITED_CONTENT",
    Spii => "SPII",
    MalformedFunctionCall => "MALFORMED_FUNCTION_CALL",
    ImageSafety => "IMAGE_SAFETY",
    ImageProhibitedContent => "IMAGE_PROHIBITED_CONTENT",
    ImageOther => "IMAGE_OTHER",
    NoImage => "NO_IMAGE",
    ImageRecitation => "IMAGE_RECITATION",
    UnexpectedToolCall => "UNEXPECTED_TOOL_CALL",
    TooManyToolCalls => "TOO_MANY_TOOL_CALLS",
    MissingThoughtSignature => "MISSING_THOUGHT_SIGNATURE",
    MalformedResponse => "MALFORMED_RESPONSE",
});

extensible_string_enum!(BlockReason, BlockReasonKnown {
    BlockReasonUnspecified => "BLOCK_REASON_UNSPECIFIED",
    Safety => "SAFETY",
    Other => "OTHER",
    Blocklist => "BLOCKLIST",
    ProhibitedContent => "PROHIBITED_CONTENT",
    ImageSafety => "IMAGE_SAFETY",
});

extensible_string_enum!(ModelStage, ModelStageKnown {
    ModelStageUnspecified => "MODEL_STAGE_UNSPECIFIED",
    UnstableExperimental => "UNSTABLE_EXPERIMENTAL",
    Experimental => "EXPERIMENTAL",
    Preview => "PREVIEW",
    Stable => "STABLE",
    Legacy => "LEGACY",
    Deprecated => "DEPRECATED",
    Retired => "RETIRED",
});

extensible_string_enum!(TaskType, TaskTypeKnown {
    TaskTypeUnspecified => "TASK_TYPE_UNSPECIFIED",
    RetrievalQuery => "RETRIEVAL_QUERY",
    RetrievalDocument => "RETRIEVAL_DOCUMENT",
    SemanticSimilarity => "SEMANTIC_SIMILARITY",
    Classification => "CLASSIFICATION",
    Clustering => "CLUSTERING",
    QuestionAnswering => "QUESTION_ANSWERING",
    FactVerification => "FACT_VERIFICATION",
    CodeRetrievalQuery => "CODE_RETRIEVAL_QUERY",
});

extensible_string_enum!(MediaResolutionLevel, MediaResolutionLevelKnown {
    MediaResolutionUnspecified => "MEDIA_RESOLUTION_UNSPECIFIED",
    MediaResolutionLow => "MEDIA_RESOLUTION_LOW",
    MediaResolutionMedium => "MEDIA_RESOLUTION_MEDIUM",
    MediaResolutionHigh => "MEDIA_RESOLUTION_HIGH",
    MediaResolutionUltraHigh => "MEDIA_RESOLUTION_ULTRA_HIGH",
});

extensible_string_enum!(GenerationMediaResolution, GenerationMediaResolutionKnown {
    MediaResolutionUnspecified => "MEDIA_RESOLUTION_UNSPECIFIED",
    MediaResolutionLow => "MEDIA_RESOLUTION_LOW",
    MediaResolutionMedium => "MEDIA_RESOLUTION_MEDIUM",
    MediaResolutionHigh => "MEDIA_RESOLUTION_HIGH",
});

extensible_string_enum!(ThinkingLevel, ThinkingLevelKnown {
    ThinkingLevelUnspecified => "THINKING_LEVEL_UNSPECIFIED",
    Minimal => "MINIMAL",
    Low => "LOW",
    Medium => "MEDIUM",
    High => "HIGH",
});

extensible_string_enum!(FunctionResponseScheduling, FunctionResponseSchedulingKnown {
    SchedulingUnspecified => "SCHEDULING_UNSPECIFIED",
    Silent => "SILENT",
    WhenIdle => "WHEN_IDLE",
    Interrupt => "INTERRUPT",
});

extensible_string_enum!(ExecutableCodeLanguage, ExecutableCodeLanguageKnown {
    LanguageUnspecified => "LANGUAGE_UNSPECIFIED",
    Python => "PYTHON",
});

extensible_string_enum!(CodeExecutionOutcome, CodeExecutionOutcomeKnown {
    OutcomeUnspecified => "OUTCOME_UNSPECIFIED",
    OutcomeOk => "OUTCOME_OK",
    OutcomeFailed => "OUTCOME_FAILED",
    OutcomeDeadlineExceeded => "OUTCOME_DEADLINE_EXCEEDED",
});

extensible_string_enum!(ServerToolType, ServerToolTypeKnown {
    ToolTypeUnspecified => "TOOL_TYPE_UNSPECIFIED",
    GoogleSearchWeb => "GOOGLE_SEARCH_WEB",
    GoogleSearchImage => "GOOGLE_SEARCH_IMAGE",
    UrlContext => "URL_CONTEXT",
    GoogleMaps => "GOOGLE_MAPS",
    FileSearch => "FILE_SEARCH",
});

extensible_string_enum!(SchemaType, SchemaTypeKnown {
    TypeUnspecified => "TYPE_UNSPECIFIED",
    String => "STRING",
    Number => "NUMBER",
    Integer => "INTEGER",
    Boolean => "BOOLEAN",
    Array => "ARRAY",
    Object => "OBJECT",
    Null => "NULL",
});

extensible_string_enum!(FunctionBehavior, FunctionBehaviorKnown {
    Unspecified => "UNSPECIFIED",
    Blocking => "BLOCKING",
    NonBlocking => "NON_BLOCKING",
});

extensible_string_enum!(DynamicRetrievalMode, DynamicRetrievalModeKnown {
    ModeUnspecified => "MODE_UNSPECIFIED",
    ModeDynamic => "MODE_DYNAMIC",
});

extensible_string_enum!(FunctionCallingMode, FunctionCallingModeKnown {
    ModeUnspecified => "MODE_UNSPECIFIED",
    Auto => "AUTO",
    Any => "ANY",
    None => "NONE",
    Validated => "VALIDATED",
});

extensible_string_enum!(ComputerUseEnvironment, ComputerUseEnvironmentKnown {
    EnvironmentUnspecified => "ENVIRONMENT_UNSPECIFIED",
    EnvironmentBrowser => "ENVIRONMENT_BROWSER",
});

extensible_string_enum!(UrlRetrievalStatus, UrlRetrievalStatusKnown {
    UrlRetrievalStatusUnspecified => "URL_RETRIEVAL_STATUS_UNSPECIFIED",
    UrlRetrievalStatusSuccess => "URL_RETRIEVAL_STATUS_SUCCESS",
    UrlRetrievalStatusError => "URL_RETRIEVAL_STATUS_ERROR",
    UrlRetrievalStatusPaywall => "URL_RETRIEVAL_STATUS_PAYWALL",
    UrlRetrievalStatusUnsafe => "URL_RETRIEVAL_STATUS_UNSAFE",
});

extensible_string_enum!(SupportedGenerationMethod, SupportedGenerationMethodKnown {
    GenerateMessage => "generateMessage",
    GenerateContent => "generateContent",
    StreamGenerateContent => "streamGenerateContent",
    CountTokens => "countTokens",
    EmbedContent => "embedContent",
    BatchEmbedContents => "batchEmbedContents",
});

extensible_string_enum!(BatchState, BatchStateKnown {
    BatchStateUnspecified => "BATCH_STATE_UNSPECIFIED",
    BatchStatePending => "BATCH_STATE_PENDING",
    BatchStateRunning => "BATCH_STATE_RUNNING",
    BatchStateSucceeded => "BATCH_STATE_SUCCEEDED",
    BatchStateFailed => "BATCH_STATE_FAILED",
    BatchStateCancelled => "BATCH_STATE_CANCELLED",
    BatchStateExpired => "BATCH_STATE_EXPIRED",
});

extensible_string_enum!(ImageAspectRatio, ImageAspectRatioKnown {
    OneToOne => "1:1",
    OneToFour => "1:4",
    FourToOne => "4:1",
    OneToEight => "1:8",
    EightToOne => "8:1",
    TwoToThree => "2:3",
    ThreeToTwo => "3:2",
    ThreeToFour => "3:4",
    FourToThree => "4:3",
    FourToFive => "4:5",
    FiveToFour => "5:4",
    NineToSixteen => "9:16",
    SixteenToNine => "16:9",
    TwentyOneToNine => "21:9",
});

extensible_string_enum!(ImageSize, ImageSizeKnown {
    Size512 => "512",
    Size1K => "1K",
    Size2K => "2K",
    Size4K => "4K",
});

extensible_string_enum!(TextResponseFormatMimeType, TextResponseFormatMimeTypeKnown {
    MimeTypeUnspecified => "MIME_TYPE_UNSPECIFIED",
    ApplicationJson => "APPLICATION_JSON",
    TextPlain => "TEXT_PLAIN",
});

extensible_string_enum!(AudioResponseFormatMimeType, AudioResponseFormatMimeTypeKnown {
    MimeTypeUnspecified => "MIME_TYPE_UNSPECIFIED",
    AudioMp3 => "AUDIO_MP3",
    AudioOggOpus => "AUDIO_OGG_OPUS",
    AudioL16 => "AUDIO_L16",
    AudioWav => "AUDIO_WAV",
    AudioAlaw => "AUDIO_ALAW",
    AudioMulaw => "AUDIO_MULAW",
});

extensible_string_enum!(AudioResponseDelivery, AudioResponseDeliveryKnown {
    DeliveryUnspecified => "DELIVERY_UNSPECIFIED",
    Inline => "INLINE",
    Uri => "URI",
});

extensible_string_enum!(ImageResponseFormatMimeType, ImageResponseFormatMimeTypeKnown {
    MimeTypeUnspecified => "MIME_TYPE_UNSPECIFIED",
    ImageJpeg => "IMAGE_JPEG",
});

extensible_string_enum!(ImageResponseDelivery, ImageResponseDeliveryKnown {
    DeliveryUnspecified => "DELIVERY_UNSPECIFIED",
    Inline => "INLINE",
    Uri => "URI",
});

extensible_string_enum!(ImageResponseAspectRatio, ImageResponseAspectRatioKnown {
    AspectRatioUnspecified => "ASPECT_RATIO_UNSPECIFIED",
    AspectRatioOneByOne => "ASPECT_RATIO_ONE_BY_ONE",
    AspectRatioTwoByThree => "ASPECT_RATIO_TWO_BY_THREE",
    AspectRatioThreeByTwo => "ASPECT_RATIO_THREE_BY_TWO",
    AspectRatioThreeByFour => "ASPECT_RATIO_THREE_BY_FOUR",
    AspectRatioFourByThree => "ASPECT_RATIO_FOUR_BY_THREE",
    AspectRatioFourByFive => "ASPECT_RATIO_FOUR_BY_FIVE",
    AspectRatioFiveByFour => "ASPECT_RATIO_FIVE_BY_FOUR",
    AspectRatioNineBySixteen => "ASPECT_RATIO_NINE_BY_SIXTEEN",
    AspectRatioSixteenByNine => "ASPECT_RATIO_SIXTEEN_BY_NINE",
    AspectRatioTwentyOneByNine => "ASPECT_RATIO_TWENTY_ONE_BY_NINE",
    AspectRatioOneByEight => "ASPECT_RATIO_ONE_BY_EIGHT",
    AspectRatioEightByOne => "ASPECT_RATIO_EIGHT_BY_ONE",
    AspectRatioOneByFour => "ASPECT_RATIO_ONE_BY_FOUR",
    AspectRatioFourByOne => "ASPECT_RATIO_FOUR_BY_ONE",
});

extensible_string_enum!(ImageResponseSize, ImageResponseSizeKnown {
    ImageSizeUnspecified => "IMAGE_SIZE_UNSPECIFIED",
    ImageSizeFiveTwelve => "IMAGE_SIZE_FIVE_TWELVE",
    ImageSizeOneK => "IMAGE_SIZE_ONE_K",
    ImageSizeTwoK => "IMAGE_SIZE_TWO_K",
    ImageSizeFourK => "IMAGE_SIZE_FOUR_K",
});

extensible_string_enum!(SpeechLanguageCode, SpeechLanguageCodeKnown {
    DeDe => "de-DE",
    EnAu => "en-AU",
    EnGb => "en-GB",
    EnIn => "en-IN",
    EnUs => "en-US",
    EsUs => "es-US",
    FrFr => "fr-FR",
    HiIn => "hi-IN",
    PtBr => "pt-BR",
    ArXa => "ar-XA",
    EsEs => "es-ES",
    FrCa => "fr-CA",
    IdId => "id-ID",
    ItIt => "it-IT",
    JaJp => "ja-JP",
    TrTr => "tr-TR",
    ViVn => "vi-VN",
    BnIn => "bn-IN",
    GuIn => "gu-IN",
    KnIn => "kn-IN",
    MlIn => "ml-IN",
    MrIn => "mr-IN",
    TaIn => "ta-IN",
    TeIn => "te-IN",
    NlNl => "nl-NL",
    KoKr => "ko-KR",
    CmnCn => "cmn-CN",
    PlPl => "pl-PL",
    RuRu => "ru-RU",
    ThTh => "th-TH",
});
