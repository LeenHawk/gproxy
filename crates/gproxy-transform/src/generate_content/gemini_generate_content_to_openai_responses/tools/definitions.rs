use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn to_responses(
    tools: Option<Vec<gemini::Tool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    let mut output = Vec::new();
    for tool in tools.into_iter().flatten() {
        for function in tool.function_declarations.into_iter().flatten() {
            output.push(function_tool(function)?);
        }
        if let Some(search) = tool.file_search {
            output.push(file_search(search)?);
        }
        if let Some(search) = web_search(
            tool.google_search,
            tool.google_search_retrieval,
            tool.url_context,
            tool.google_maps,
        )? {
            output.push(search);
        }
        if tool.code_execution.is_some() {
            output.push(openai::ResponseTool::CodeInterpreter {
                container: openai::CodeInterpreterContainer::Auto(
                    openai::CodeInterpreterAutoContainer {
                        type_: openai::CodeInterpreterContainerType::Auto,
                        file_ids: None,
                        memory_limit: None,
                        network_policy: None,
                        rest: Default::default(),
                    },
                ),
                allowed_callers: None,
                rest: Default::default(),
            });
        }
        if tool.computer_use.is_some() {
            output.push(openai::ResponseTool::Computer {
                rest: Default::default(),
            });
        }
        for server in tool.mcp_servers.into_iter().flatten() {
            output.push(super::mcp::convert(server)?);
        }
    }
    Ok((!output.is_empty()).then_some(output))
}

fn function_tool(
    function: gemini::FunctionDeclaration,
) -> Result<openai::ResponseTool, TransformError> {
    Ok(openai::ResponseTool::Function {
        name: function.name,
        parameters: function
            .parameters_json_schema
            .map(object_schema)
            .transpose()?
            .or(function.parameters.map(typed_schema).transpose()?)
            .map(openai::ResponseFunctionParameters::Schema)
            .unwrap_or(openai::ResponseFunctionParameters::Null),
        strict: openai::ResponseFunctionStrict::Absent,
        defer_loading: None,
        description: Some(function.description),
        output_schema: None,
        allowed_callers: None,
        rest: Default::default(),
    })
}

fn file_search(search: gemini::FileSearch) -> Result<openai::ResponseTool, TransformError> {
    Ok(openai::ResponseTool::FileSearch {
        vector_store_ids: search.file_search_store_names,
        filters: None,
        max_num_results: search.top_k.map(nonnegative).transpose()?,
        ranking_options: None,
        rest: Default::default(),
    })
}

fn web_search(
    search: Option<gemini::GoogleSearch>,
    retrieval: Option<gemini::GoogleSearchRetrieval>,
    url: Option<gemini::UrlContext>,
    maps: Option<gemini::GoogleMaps>,
) -> Result<Option<openai::ResponseTool>, TransformError> {
    let hosted_search = search.is_some() || retrieval.is_some();
    if !hosted_search && url.is_none() && maps.is_none() {
        return Ok(None);
    }
    let _ = retrieval;
    let _ = url;
    let _ = maps;
    let search_content_types = search.map(search_types).transpose()?.flatten();
    Ok(Some(if search_content_types.is_some() {
        openai::ResponseTool::WebSearchPreview {
            search_content_types,
            search_context_size: None,
            user_location: None,
            rest: Default::default(),
        }
    } else {
        openai::ResponseTool::WebSearch {
            filters: None,
            max_uses: None,
            search_context_size: None,
            user_location: None,
            rest: Default::default(),
        }
    }))
}

fn search_types(
    search: gemini::GoogleSearch,
) -> Result<Option<Vec<openai::SearchContentType>>, TransformError> {
    let Some(types) = search.search_types else {
        return Ok(None);
    };
    let mut output = Vec::new();
    if let Some(value) = types.web_search {
        let _ = value;
        output.push(openai::SearchContentType::Text);
    }
    if let Some(value) = types.image_search {
        let _ = value;
        output.push(openai::SearchContentType::Image);
    }
    Ok(Some(output))
}

fn typed_schema(schema: gemini::Schema) -> Result<openai::JsonSchema, TransformError> {
    let mut value = serde_json::to_value(schema)?;
    crate::common::gemini_schema::normalize(&mut value);
    object_schema(value)
}

fn object_schema(value: serde_json::Value) -> Result<openai::JsonSchema, TransformError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("Gemini function schema", "expected an object"))
}

fn nonnegative(value: i32) -> Result<u32, TransformError> {
    u32::try_from(value)
        .map_err(|_| TransformError::shape("Gemini fileSearch tool", "topK must be nonnegative"))
}
