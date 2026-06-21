use crate::protocol::gemini;

pub(in crate::transform::count_tokens) struct GeminiCountTokenParts {
    pub model: Option<String>,
    pub contents: Vec<gemini::Content>,
    pub system_instruction: Option<gemini::Content>,
    pub tools: Vec<gemini::Tool>,
    pub tool_config: Option<gemini::ToolConfig>,
    pub generation_config: Option<gemini::GenerationConfig>,
    pub service_tier: Option<gemini::ServiceTier>,
}

pub(in crate::transform::count_tokens) fn split_gemini_count_token_request(
    input: gemini::CountTokensRequest,
) -> GeminiCountTokenParts {
    let mut model = input.model;
    let mut contents = input.contents;
    let mut system_instruction = None;
    let mut tools = Vec::new();
    let mut tool_config = None;
    let mut generation_config = None;
    let mut service_tier = None;

    if let Some(request) = input.generate_content_request {
        if model.is_none() {
            model = request.model;
        }
        if contents.is_empty() {
            contents = request.contents;
        }
        system_instruction = request.system_instruction;
        tools = request.tools;
        tool_config = request.tool_config;
        generation_config = request.generation_config;
        service_tier = request.service_tier;
    }

    GeminiCountTokenParts {
        model,
        contents,
        system_instruction,
        tools,
        tool_config,
        generation_config,
        service_tier,
    }
}
