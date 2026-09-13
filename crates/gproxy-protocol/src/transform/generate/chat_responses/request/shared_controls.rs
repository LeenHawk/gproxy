use crate::{
    transform::TransformError,
    wire::openai::{chat as c, responses::generate as r},
};
pub(super) fn to_responses(
    input: &c::GenerateContentRequestBody,
    out: &mut r::GenerateContentRequestBody,
) -> Result<(), TransformError> {
    out.prompt_cache_retention = input.prompt_cache_retention.map(|v| {
        v.map(|v| match v {
            c::PromptCacheRetention::InMemory => r::PromptCacheRetention::InMemory,
            c::PromptCacheRetention::TwentyFourHours => r::PromptCacheRetention::TwentyFourHours,
        })
    });
    out.prompt_cache_options = input.prompt_cache_options.as_ref().map(|value| {
        let mut target = r::PromptCacheOptions::builder().build();
        target.mode = value.mode.map(|v| match v {
            c::PromptCacheOptionMode::Implicit => r::PromptCachingMode::Implicit,
            c::PromptCacheOptionMode::Explicit => r::PromptCachingMode::Explicit,
        });
        target.ttl = value.ttl.map(|v| match v {
            c::PromptCacheTtl::ThirtyMinutes => r::PromptCacheTtl::ThirtyMinutes,
        });
        target
    });
    out.moderation = input.moderation.as_ref().map(|value| {
        value.as_ref().map(|value| {
            let mut target = r::ResponseModeration::builder(value.model.clone()).build();
            target.policy = value.policy.as_ref().map(|value| {
                value.as_ref().map(|value| {
                    let mut policy = r::ModerationPolicy::builder().build();
                    policy.input = value.input.as_ref().map(|v| {
                        v.as_ref().map(|v| {
                            r::ModerationPolicyMode::builder(match v.mode {
                                c::ModerationModeType::Score => r::ModerationMode::Score,
                                c::ModerationModeType::Block => r::ModerationMode::Block,
                            })
                            .build()
                        })
                    });
                    policy.output = value.output.as_ref().map(|v| {
                        v.as_ref().map(|v| {
                            r::ModerationPolicyMode::builder(match v.mode {
                                c::ModerationModeType::Score => r::ModerationMode::Score,
                                c::ModerationModeType::Block => r::ModerationMode::Block,
                            })
                            .build()
                        })
                    });
                    policy
                })
            });
            target
        })
    });
    Ok(())
}
pub(super) fn to_chat(
    input: &r::GenerateContentRequestBody,
    out: &mut c::GenerateContentRequestBody,
) -> Result<(), TransformError> {
    out.prompt_cache_retention = input.prompt_cache_retention.map(|v| {
        v.map(|v| match v {
            r::PromptCacheRetention::InMemory => c::PromptCacheRetention::InMemory,
            r::PromptCacheRetention::TwentyFourHours => c::PromptCacheRetention::TwentyFourHours,
        })
    });
    out.prompt_cache_options = input.prompt_cache_options.as_ref().map(|value| {
        let mut target = c::PromptCacheOptions::builder().build();
        target.mode = value.mode.map(|v| match v {
            r::PromptCachingMode::Implicit => c::PromptCacheOptionMode::Implicit,
            r::PromptCachingMode::Explicit => c::PromptCacheOptionMode::Explicit,
        });
        target.ttl = value.ttl.map(|v| match v {
            r::PromptCacheTtl::ThirtyMinutes => c::PromptCacheTtl::ThirtyMinutes,
        });
        target
    });
    out.moderation = input.moderation.as_ref().map(|value| {
        value.as_ref().map(|value| {
            let mut target = c::Moderation::builder(value.model.clone()).build();
            target.policy = value.policy.as_ref().map(|value| {
                value.as_ref().map(|value| {
                    let mut policy = c::ModerationPolicy::builder().build();
                    policy.input = value.input.as_ref().map(|v| {
                        v.as_ref().map(|v| {
                            c::ModerationMode::builder(match v.mode {
                                r::ModerationMode::Score => c::ModerationModeType::Score,
                                r::ModerationMode::Block => c::ModerationModeType::Block,
                            })
                            .build()
                        })
                    });
                    policy.output = value.output.as_ref().map(|v| {
                        v.as_ref().map(|v| {
                            c::ModerationMode::builder(match v.mode {
                                r::ModerationMode::Score => c::ModerationModeType::Score,
                                r::ModerationMode::Block => c::ModerationModeType::Block,
                            })
                            .build()
                        })
                    });
                    policy
                })
            });
            target
        })
    });
    Ok(())
}
