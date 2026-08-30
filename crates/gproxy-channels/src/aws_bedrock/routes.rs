use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(xform ListModels, gemini => ListModels, openai),
    route!(pass GetModel, openai),
    route!(xform GetModel, claude => GetModel, openai),
    route!(xform GetModel, gemini => GetModel, openai),
    route!(xform CountTokens, openai => CountTokens, claude),
    route!(pass CountTokens, claude),
    route!(xform CountTokens, gemini => CountTokens, claude),
    route!(xform GenerateContent, openai_responses => GenerateContent, claude_messages),
    route!(xform GenerateContent, openai_chat => GenerateContent, claude_messages),
    route!(pass GenerateContent, claude_messages),
    route!(xform GenerateContent, gemini_generate_content => GenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_responses => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_chat => StreamGenerateContent, claude_messages),
    route!(pass StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, claude_messages),
    route!(xform CompactContent, openai => GenerateContent, claude_messages),
    route!(pass CreateVideo, openai),
    route!(pass RetrieveVideo, openai),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
];
