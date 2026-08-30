use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(local ListModels, claude),
    route!(local ListModels, openai),
    route!(local ListModels, gemini),
    route!(local GetModel, claude),
    route!(local GetModel, openai),
    route!(local GetModel, gemini),
    route!(local CountTokens, claude),
    route!(local CountTokens, openai),
    route!(local CountTokens, gemini),
    route!(xform GenerateContent, claude_messages => StreamGenerateContent, claude_messages),
    route!(xform GenerateContent, openai_chat => StreamGenerateContent, claude_messages),
    route!(xform GenerateContent, openai_responses => StreamGenerateContent, claude_messages),
    route!(xform GenerateContent, gemini_generate_content => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_chat => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_responses => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, claude_messages),
    route!(pass StreamGenerateContent, claude_messages),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, claude_messages),
];
