use gproxy_protocol::{gemini, openai};

use crate::TransformError;

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
        if let Some(computer) = tool.computer_use {
            ensure_empty(&computer.rest, "Gemini computerUse tool")?;
            if computer.excluded_predefined_functions.is_some() {
                return Err(TransformError::unsupported(
                    "Gemini computerUse tool",
                    "excludedPredefinedFunctions",
                ));
            }
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
    if search.metadata_filter.is_some() || !search.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini fileSearch tool",
            "metadataFilter or extension fields",
        ));
    }
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
    Ok(Some(if hosted_search {
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
