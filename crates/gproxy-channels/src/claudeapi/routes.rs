use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, claude),
    route!(pass ListModels, openai),
    route!(xform ListModels, gemini => ListModels, claude),
    route!(pass GetModel, claude),
    route!(pass GetModel, openai),
    route!(xform GetModel, gemini => GetModel, claude),
    route!(pass CountTokens, claude),
    route!(xform CountTokens, openai => CountTokens, claude),
    route!(xform CountTokens, gemini => CountTokens, claude),
    route!(pass GenerateContent, claude_messages),
    route!(pass GenerateContent, openai_chat),
    route!(xform GenerateContent, openai_responses => GenerateContent, claude_messages),
    route!(xform GenerateContent, gemini_generate_content => GenerateContent, claude_messages),
    route!(pass StreamGenerateContent, claude_messages),
    route!(pass StreamGenerateContent, openai_chat),
    route!(xform StreamGenerateContent, openai_responses => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, claude_messages),
    route!(xform CompactContent, openai => GenerateContent, claude_messages),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
];
