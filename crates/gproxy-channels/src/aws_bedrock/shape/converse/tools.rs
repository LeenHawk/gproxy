use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};
use serde_json::json;

pub(super) fn config(
    tools: Option<Vec<claude::Tool>>,
    choice: Option<claude::ToolChoice>,
) -> Result<Option<aws::ToolConfiguration>, ChannelError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        let mut value =
            serde_json::to_value(tool).map_err(|error| prepare(format!("Claude tool: {error}")))?;
        let root = value
            .as_object_mut()
            .ok_or_else(|| prepare("Claude tool is not an object"))?;
        let cache = root
            .remove("cache_control")
            .map(serde_json::from_value::<claude::CacheControl>)
            .transpose()
            .map_err(|error| prepare(format!("Claude tool cache control: {error}")))?;
        let name = root
            .remove("name")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| prepare("Claude tool has no name"))?;
        let description = root
            .remove("description")
            .and_then(|value| value.as_str().map(str::to_owned));
        let schema = root
            .remove("input_schema")
            .unwrap_or_else(|| json!({"type":"object"}));
        output.push(aws::Tool::ToolSpec {
            tool_spec: aws::ToolSpecification {
                name,
                description,
                input_schema: aws::ToolInputSchema::Json {
                    json: schema,
                    rest: Default::default(),
                },
                strict: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        });
        if let Some(cache) = cache {
            output.push(aws::Tool::CachePoint {
                cache_point: super::content::cache_point(cache),
                rest: Default::default(),
            });
        }
    }
    if output.is_empty() {
        return Ok(None);
    }
    Ok(Some(aws::ToolConfiguration {
        tools: output,
        tool_choice: choice.map(tool_choice).transpose()?.flatten(),
        rest: Default::default(),
    }))
}

fn tool_choice(choice: claude::ToolChoice) -> Result<Option<aws::ToolChoice>, ChannelError> {
    let empty = || aws::EmptyObject::default();
    Ok(Some(match choice {
        claude::ToolChoice::Auto(_) => aws::ToolChoice::Auto {
            auto: empty(),
            rest: Default::default(),
        },
        claude::ToolChoice::Any(_) => aws::ToolChoice::Any {
            any: empty(),
            rest: Default::default(),
        },
        claude::ToolChoice::Tool(choice) => aws::ToolChoice::Specific {
            tool: aws::SpecificToolChoice {
                name: choice.name,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        claude::ToolChoice::None(_) => return Ok(None),
        claude::ToolChoice::Unknown(_) => {
            return Err(prepare("unsupported Claude tool choice"));
        }
        _ => return Err(prepare("unsupported Claude tool choice variant")),
    }))
}

fn prepare(message: impl Into<String>) -> ChannelError {
    ChannelError::Prepare(message.into())
}
