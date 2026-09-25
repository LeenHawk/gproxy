//! Codex standalone text searches over a provider's hosted search tool.
//! Native encrypted search state and non-search browser commands are not
//! fabricated; callers receive the actual generated search text.

use crate::{
    openai::{responses as r, web_search as w},
    transform::TransformError,
};

pub fn to_generation(
    input: w::WebSearchRequestBody,
    model: String,
) -> Result<r::GenerateContentRequestBody, TransformError> {
    let queries = input
        .commands
        .and_then(|commands| commands.search_query)
        .filter(|queries| !queries.is_empty())
        .ok_or_else(|| {
            TransformError::unsupported(
                "web_search.commands",
                "this provider supports standalone search_query commands",
            )
        })?;
    let mut request = r::GenerateContentRequestBody::builder().build();
    request.model = Some(model);
    request.instructions = Some(Some(
        "Use the provided web search tool to answer these search queries. Return concise findings with the actual source URLs. Do not answer from memory in place of searching.".into(),
    ));
    request.input = Some(r::Input::Text(serde_json::to_string(&queries)?));
    request.tools = Some(vec![r::Tool::WebSearchLegacy(
        r::WebSearchTool::builder().build(),
    )]);
    request.max_output_tokens = input
        .max_output_tokens
        .and_then(|value| i64::try_from(value).ok())
        .map(Some);
    request.stream = Some(Some(false));
    Ok(request)
}

pub fn from_generation(
    response: r::GenerateContentResponseBody,
) -> Result<w::WebSearchResponseBody, TransformError> {
    let output = response
        .output
        .into_iter()
        .filter_map(|item| match item {
            r::ResponseOutputItem::Message(message) => Some(message.content),
            _ => None,
        })
        .flatten()
        .filter_map(|content| match content {
            r::OutputContent::Text(text) => Some(text.text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if output.trim().is_empty() {
        return Err(TransformError::invalid_result(
            "web_search.output",
            "upstream returned no search text",
        ));
    }
    Ok(w::WebSearchResponseBody::builder(output).build())
}
