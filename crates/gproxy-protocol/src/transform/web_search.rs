//! Codex search/browser commands over provider-hosted search and page tools.
//! Commands and caller-supplied context travel together so URL and reference
//! based followups can be interpreted by the model. Native encrypted browser
//! state is not portable; unavailable commands are omitted, not fabricated.

use crate::{
    openai::{responses as r, web_search as w},
    transform::TransformError,
};

pub fn to_generation(
    input: w::WebSearchRequestBody,
    model: String,
) -> Result<r::GenerateContentRequestBody, TransformError> {
    let mut request = r::GenerateContentRequestBody::builder().build();
    request.model = Some(model);
    request.instructions = Some(Some(
        "Execute the supplied web commands using the available search and page-reading tools. Use input as prior context to resolve page/result references. search_query searches the web; open reads the referenced URL/page (respect lineno when available); find locates the pattern in that page; click follows the specified link from the referenced page. Use the other commands only when the available tools support them. Preserve query domains, recency, and requested response length where supported. Skip commands or opaque references that cannot be resolved; never invent page contents, link IDs, screenshots, or tool results. Return actual findings with source URLs, using URLs as reusable page references. Do not answer from memory in place of searching or reading.".into(),
    ));
    request.input = Some(r::Input::Text(serde_json::to_string(&serde_json::json!({
        "commands": input.commands,
        "input": input.input,
        "settings": input.settings,
    }))?));
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
    Ok(w::WebSearchResponseBody::builder(output).build())
}
