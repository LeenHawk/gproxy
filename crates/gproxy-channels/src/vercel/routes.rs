use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(xform ListModels, gemini => ListModels, openai),
    route!(pass GetModel, openai),
    route!(xform GetModel, claude => GetModel, openai),
    route!(xform GetModel, gemini => GetModel, openai),
    route!(pass CountTokens, claude),
    route!(xform CountTokens, openai => CountTokens, claude),
    route!(xform CountTokens, gemini => CountTokens, claude),
    route!(pass GenerateContent, openai_responses),
    route!(pass GenerateContent, openai_chat),
    route!(pass GenerateContent, claude_messages),
    route!(xform GenerateContent, gemini_generate_content => GenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_chat),
    route!(pass StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, openai_responses),
    route!(pass CreateEmbedding, openai),
    route!(xform CreateEmbedding, gemini => CreateEmbedding, openai),
    route!(xform CompactContent, openai => GenerateContent, openai_responses),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
];
