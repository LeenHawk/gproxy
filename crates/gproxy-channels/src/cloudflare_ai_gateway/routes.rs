use gproxy_channel_api::ChannelSupport;

use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(local CountTokens, openai),
    route!(local CountTokens, claude),
    route!(local CountTokens, gemini),
    route!(pass GenerateContent, openai_responses),
    route!(pass GenerateContent, openai_chat),
    route!(pass GenerateContent, claude_messages),
    route!(xform GenerateContent, gemini_generate_content => GenerateContent, openai_chat),
    route!(pass StreamGenerateContent, openai_responses),
    route!(pass StreamGenerateContent, openai_chat),
    route!(pass StreamGenerateContent, claude_messages),
    route!(xform StreamGenerateContent, gemini_generate_content => StreamGenerateContent, openai_chat),
    route!(xform CompactContent, openai => GenerateContent, openai_responses),
];
