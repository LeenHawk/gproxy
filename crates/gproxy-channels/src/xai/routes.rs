use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(xform ListModels, gemini => ListModels, openai),
    route!(pass GetModel, openai),
    route!(xform GetModel, claude => GetModel, openai),
    route!(xform GetModel, gemini => GetModel, openai),
    route!(local CountTokens, openai),
    route!(local CountTokens, claude),
    route!(local CountTokens, gemini),
    route!(pass GenerateContent, openai_responses),
    route!(pass GenerateContent, openai_chat),
    route!(xform GenerateContent, claude_messages => GenerateContent, openai_responses),
    route!(xform GenerateContent, gemini_generate_content => GenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_chat),
    route!(xform StreamGenerateContent, claude_messages => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, openai_responses),
    route!(pass CreateImage, openai),
    route!(pass EditImage, openai),
    route!(pass CreateSpeech, openai),
    route!(pass CreateTranscription, openai),
    route!(pass CreateVideo, openai),
    route!(pass RetrieveVideo, openai),
    route!(pass EditVideo, openai),
    route!(pass ExtendVideo, openai),
    route!(pass CompactContent, openai),
    route!(xform GenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
    route!(xform StreamGenerateContent, openai_responses_websocket => StreamGenerateContent, openai_responses),
];
