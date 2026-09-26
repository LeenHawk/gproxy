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

impl ChatViaClaudeFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> ch::ClaudeToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<ch::ClaudeToChatStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input.n.flatten().unwrap_or(1) {
            let identities = target.options.child_identity(index, crate::Dialect::Claude);
            let context = context(index as usize);
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaClaude::prepare_stream(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> ch::ClaudeToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<ch::ClaudeToChatStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input.n.flatten().unwrap_or(1) {
            let identities = target.options.child_identity(index, crate::Dialect::Claude);
            let context = context(index as usize);
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaClaude::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
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

        let mut children = Vec::new();
        for index in 0..input.n.flatten().unwrap_or(1) {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaResponses::prepare_stream(
                    request,
                    StreamTarget {
                        endpoint: target.endpoint.clone(),
                        identities,
                    },
                    settings,
                    state,
                )
                .await?,
            );
        }
        FanoutStream::new(children, target.options, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<hr::ResponsesToChatStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input.n.flatten().unwrap_or(1) {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            let mut request = input.clone();
            request.n = Some(Some(1));
            children.push(
                ChatViaResponses::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
}

impl GeminiViaClaudeFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<cg::ClaudeToGeminiStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input
            .generation_config
            .as_ref()
            .and_then(|v| v.candidate_count)
            .unwrap_or(1)
        {
            let identities = target.options.child_identity(index, crate::Dialect::Claude);
            let context = context(index as usize);
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaClaude::prepare_stream(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<cg::ClaudeToGeminiStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input
            .generation_config
            .as_ref()
            .and_then(|v| v.candidate_count)
            .unwrap_or(1)
        {
            let identities = target.options.child_identity(index, crate::Dialect::Claude);
            let context = context(index as usize);
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaClaude::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
}

impl GeminiViaResponsesFanout {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> gr::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<FanoutStream<gr::ResponsesToGeminiStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input
            .generation_config
            .as_ref()
            .and_then(|v| v.candidate_count)
            .unwrap_or(1)
        {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            let context = context(index as usize);
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaResponses::prepare_stream(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        mut context: impl FnMut(usize) -> gr::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<FanoutStream<gr::ResponsesToGeminiStream>, TransformError> {
        let input = input.into_declared();

        let mut children = Vec::new();
        for index in 0..input
            .generation_config
            .as_ref()
            .and_then(|v| v.candidate_count)
            .unwrap_or(1)
        {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            let context = context(index as usize);
            let mut request = input.clone();
            request
                .generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .candidate_count = Some(1);
            children.push(
                GeminiViaResponses::prepare_stream_with_capabilities(
                    request,
                    StreamTarget {
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
        FanoutStream::new(children, target.options, state).await
    }
}
