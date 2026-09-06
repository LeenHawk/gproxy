use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(local ListModels, gemini),
    route!(local ListModels, claude),
    route!(local ListModels, openai),
    route!(local GetModel, gemini),
    route!(local GetModel, claude),
    route!(local GetModel, openai),
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
    route!(unsupported CreateEmbedding, gemini),
    route!(unsupported CreateEmbedding, openai),
    route!(xform CompactContent, openai => GenerateContent, gemini_generate_content),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, gemini_generate_content),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, gemini_generate_content),
];
