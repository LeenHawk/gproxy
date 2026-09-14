use super::super::{GeminiViaClaudeStreamFacts, StreamTarget};
use super::*;
use crate::{
    adapt::generate::{
        GenerationResources,
        chat_claude::ChatViaClaude,
        chat_responses::ChatViaResponses,
        claude_gemini::GeminiViaClaude,
        fanout::{
            ChatViaClaudeFanout, ChatViaResponsesFanout, FanoutTarget, GeminiViaClaudeFanout,
            GeminiViaResponsesFanout,
        },
        gemini_responses::GeminiViaResponses,
    },
    capability::ResourceAccess,
    transform::generate::{
        chat_responses::stream as hr, claude_chat::stream as ch, claude_gemini::stream as cg,
        gemini_responses::stream as gr,
    },
    wire::{DeclaredFields, gemini as g, openai::chat as h},
};
fn validate(
    target: &FanoutTarget,
    count: i64,
    settings: StreamSettings,
) -> Result<(), TransformError> {
    target.endpoint.validate()?;
    if usize::try_from(count).ok() != Some(target.identities.len()) {
        return Err(TransformError::shape(
            "fanout.count",
            "candidate count must match distinct prepared child identities",
        ));
    }
    group_id(target.options, &target.identities)?;
    if target.identities.len() > settings.events.max_choices {
        return Err(limit("fanout candidate limit exceeded"));
    }
    Ok(())
}
impl ChatViaClaudeFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<ch::ClaudeToChatContext>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<ch::ClaudeToChatStream>, TransformError> {
        let input = input.into_declared();
        validate(&target, input.n.flatten().unwrap_or(1), settings)?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaClaude::prepare_stream(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<ch::ClaudeToChatContext>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<ch::ClaudeToChatStream>, TransformError> {
        let input = input.into_declared();
        validate(&target, input.n.flatten().unwrap_or(1), settings)?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaClaude::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                    resources,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
}
impl ChatViaResponsesFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<hr::ResponsesToChatStream>, TransformError> {
        let input = input.into_declared();
        validate(&target, input.n.flatten().unwrap_or(1), settings)?;
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for identities in target.identities {
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaResponses::prepare_stream(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    settings,
                    state,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<hr::ResponsesToChatStream>, TransformError> {
        let input = input.into_declared();
        validate(&target, input.n.flatten().unwrap_or(1), settings)?;
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for identities in target.identities {
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaResponses::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    settings,
                    state,
                    resources,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
}
impl GeminiViaClaudeFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<GeminiViaClaudeStreamFacts>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<cg::ClaudeToGeminiStream>, TransformError> {
        let input = input.into_declared();
        validate(
            &target,
            input
                .generation_config
                .as_ref()
                .and_then(|c| c.candidate_count)
                .unwrap_or(1),
            settings,
        )?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaClaude::prepare_stream(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<GeminiViaClaudeStreamFacts>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<cg::ClaudeToGeminiStream>, TransformError> {
        let input = input.into_declared();
        validate(
            &target,
            input
                .generation_config
                .as_ref()
                .and_then(|c| c.candidate_count)
                .unwrap_or(1),
            settings,
        )?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaClaude::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                    resources,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
}
impl GeminiViaResponsesFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<gr::ResponsesToGeminiContext>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<gr::ResponsesToGeminiStream>, TransformError> {
        let input = input.into_declared();
        validate(
            &target,
            input
                .generation_config
                .as_ref()
                .and_then(|c| c.candidate_count)
                .unwrap_or(1),
            settings,
        )?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaResponses::prepare_stream(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        contexts: Vec<gr::ResponsesToGeminiContext>,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<gr::ResponsesToGeminiStream>, TransformError> {
        let input = input.into_declared();
        validate(
            &target,
            input
                .generation_config
                .as_ref()
                .and_then(|c| c.candidate_count)
                .unwrap_or(1),
            settings,
        )?;
        if contexts.len() != target.identities.len() {
            return Err(TransformError::shape(
                "fanout.contexts",
                "each child requires its factual response context",
            ));
        }
        let original = crate::codec::encode_json(&input, settings.codec)
            .map_err(codec_error)?
            .to_vec();
        let mut children = Vec::new();
        for (identities, context) in target.identities.into_iter().zip(contexts) {
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaResponses::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
                        model: target.model.clone(),
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    context,
                    settings,
                    state,
                    resources,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, original, state).await
    }
}
