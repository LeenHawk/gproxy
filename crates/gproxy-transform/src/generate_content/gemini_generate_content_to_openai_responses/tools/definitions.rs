use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::common::tools::empty_response_tool;

pub(crate) fn to_responses(
    tools: Option<Vec<gemini::Tool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    let mut output = Vec::new();
    for tool in tools.into_iter().flatten() {
        ensure_empty(&tool.rest, "Gemini tool")?;
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
        if let Some(code) = tool.code_execution {
            ensure_empty(&code.rest, "Gemini codeExecution tool")?;
            output.push(openai::ResponseTool {
                type_: openai::ToolType::Shell,
                ..empty_response_tool()
            });
        }
        if let Some(computer) = tool.computer_use {
            ensure_empty(&computer.rest, "Gemini computerUse tool")?;
            if computer.excluded_predefined_functions.is_some() {
                return Err(TransformError::unsupported(
                    "Gemini computerUse tool",
                    "excludedPredefinedFunctions",
                ));
            }
            output.push(openai::ResponseTool {
                type_: openai::ToolType::ComputerUse,
                environment: computer.environment.map(serde_json::to_value).transpose()?,
                ..empty_response_tool()
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
    if function.behavior.is_some()
        || function.response.is_some()
        || function.response_json_schema.is_some()
        || !function.rest.is_empty()
    {
        return Err(TransformError::unsupported(
            "Gemini function declaration",
            "behavior, response schema, or extension fields",
        ));
    }
    if function.parameters.is_some() && function.parameters_json_schema.is_some() {
        return Err(TransformError::shape(
            "Gemini function declaration",
            "both parameter schema forms are present",
        ));
    }
    Ok(openai::ResponseTool {
        type_: openai::ToolType::Function,
        name: Some(function.name),
        parameters: function
            .parameters_json_schema
            .map(object_schema)
            .transpose()?
            .or(function.parameters.map(typed_schema).transpose()?),
        description: Some(function.description),
        ..empty_response_tool()
    })
}

fn file_search(search: gemini::FileSearch) -> Result<openai::ResponseTool, TransformError> {
    if search.metadata_filter.is_some() || !search.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini fileSearch tool",
            "metadataFilter or extension fields",
        ));
    }
    Ok(openai::ResponseTool {
        type_: openai::ToolType::FileSearch,
        vector_store_ids: Some(search.file_search_store_names),
        max_num_results: search.top_k.map(nonnegative).transpose()?,
        ..empty_response_tool()
    })
}

fn web_search(
    search: Option<gemini::GoogleSearch>,
    retrieval: Option<gemini::GoogleSearchRetrieval>,
    url: Option<gemini::UrlContext>,
    maps: Option<gemini::GoogleMaps>,
) -> Result<Option<openai::ResponseTool>, TransformError> {
    if search.is_none() && retrieval.is_none() && url.is_none() && maps.is_none() {
        return Ok(None);
    }
    if let Some(value) = retrieval
        && (value.dynamic_retrieval_config.is_some() || !value.rest.is_empty())
    {
        return Err(TransformError::unsupported(
            "Gemini googleSearchRetrieval tool",
            "dynamic retrieval or extension settings",
        ));
    }
    if let Some(value) = url {
        ensure_empty(&value.rest, "Gemini urlContext tool")?;
    }
    if let Some(value) = maps
        && (value.enable_widget.is_some() || !value.rest.is_empty())
    {
        return Err(TransformError::unsupported(
            "Gemini googleMaps tool",
            "widget or extension settings",
        ));
    }
    let search_content_types = search.map(search_types).transpose()?.flatten();
    Ok(Some(openai::ResponseTool {
        type_: openai::ToolType::WebSearch,
        search_content_types,
        ..empty_response_tool()
    }))
}

fn search_types(
    search: gemini::GoogleSearch,
) -> Result<Option<Vec<openai::SearchContentType>>, TransformError> {
    if search.time_range_filter.is_some() || !search.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini googleSearch tool",
            "time range or extension settings",
        ));
    }
    let Some(types) = search.search_types else {
        return Ok(None);
    };
    ensure_empty(&types.rest, "Gemini searchTypes")?;
    let mut output = Vec::new();
    if let Some(value) = types.web_search {
        ensure_empty(&value.rest, "Gemini webSearch selector")?;
        output.push(openai::SearchContentType::Text);
    }
    if let Some(value) = types.image_search {
        ensure_empty(&value.rest, "Gemini imageSearch selector")?;
        output.push(openai::SearchContentType::Image);
    }
    Ok(Some(output))
}

fn typed_schema(schema: gemini::Schema) -> Result<openai::JsonSchema, TransformError> {
    object_schema(serde_json::to_value(schema)?)
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

pub(super) fn ensure_empty(
    rest: &gemini::JsonMap,
    wire: &'static str,
) -> Result<(), TransformError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(TransformError::unsupported(wire, "extension fields"))
    }
}
