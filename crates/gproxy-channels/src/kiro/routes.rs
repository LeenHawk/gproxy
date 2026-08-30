use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(xform ListModels, gemini => ListModels, openai),
    route!(local CountTokens, openai),
    route!(local CountTokens, claude),
    route!(local CountTokens, gemini),
    route!(xform GenerateContent, openai_responses => StreamGenerateContent, openai_responses),
    route!(xform GenerateContent, openai_chat => StreamGenerateContent, openai_responses),
    route!(xform GenerateContent, claude_messages => StreamGenerateContent, openai_responses),
    route!(xform GenerateContent, gemini_generate_content => StreamGenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, openai_chat => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, claude_messages => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, openai_responses),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
];
