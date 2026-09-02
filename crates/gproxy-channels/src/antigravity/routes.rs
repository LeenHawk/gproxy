use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, gemini),
    route!(xform ListModels, claude => ListModels, gemini),
    route!(xform ListModels, openai => ListModels, gemini),
    route!(pass GetModel, gemini),
    route!(xform GetModel, claude => GetModel, gemini),
    route!(xform GetModel, openai => GetModel, gemini),
    route!(pass CountTokens, gemini),
    route!(xform CountTokens, claude => CountTokens, gemini),
    route!(xform CountTokens, openai => CountTokens, gemini),
    route!(pass GenerateContent, gemini_generate_content),
    route!(xform GenerateContent, claude_messages => GenerateContent, gemini_generate_content),
    route!(xform GenerateContent, openai_chat => GenerateContent, gemini_generate_content),
    route!(xform GenerateContent, openai_responses => GenerateContent, gemini_generate_content),
    route!(pass StreamGenerateContent, gemini_generate_content),
    route!(xform StreamGenerateContent, claude_messages => StreamGenerateContent, gemini_generate_content),
    route!(xform StreamGenerateContent, openai_chat => StreamGenerateContent, gemini_generate_content),
    route!(xform StreamGenerateContent, openai_responses => StreamGenerateContent, gemini_generate_content),
    route!(xform CreateImage, openai => StreamGenerateContent, gemini_generate_content),
    route!(xform EditImage, openai => StreamGenerateContent, gemini_generate_content),
    route!(xform CompactContent, openai => GenerateContent, gemini_generate_content),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, gemini_generate_content),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, gemini_generate_content),
];
