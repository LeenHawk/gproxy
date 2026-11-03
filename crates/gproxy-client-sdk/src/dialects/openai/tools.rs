use super::*;
pub(super) fn server_output(
    id: String,
    name: &str,
    output: Value,
    kind: ServerOutputKind,
) -> Result<Block> {
    with_native(
        Content::ToolResult(ToolResult {
            id,
            name: name.into(),
            output: ToolOutput::Json(output),
            is_error: false,
            provider_executed: true,
        }),
        Vendor::Openai,
        ServerOutputMetadata { kind },
    )
}

pub(super) fn restore_server_output(
    block: &Block,
    result: &ToolResult,
    input: &mut [i::InputItem],
) -> Result<()> {
    let Some(meta) = read_native::<ServerOutputMetadata>(block, Vendor::Openai)? else {
        return Ok(());
    };
    let ToolOutput::Json(output) = &result.output else {
        return Err(Error::invalid("native server output requires JSON"));
    };
    match meta.kind {
        ServerOutputKind::FileSearch => {
            let target = input
                .iter_mut()
                .rev()
                .find_map(|item| match item {
                    i::InputItem::FileSearchCall(value) if value.id == result.id => Some(value),
                    _ => None,
                })
                .ok_or_else(|| Error::invalid("file-search result has no earlier native call"))?;
            target.results = Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
        }
        ServerOutputKind::CodeInterpreter => {
            let target = input
                .iter_mut()
                .rev()
                .find_map(|item| match item {
                    i::InputItem::CodeInterpreterCall(value) if value.id == result.id => {
                        Some(value)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    Error::invalid("code-interpreter result has no earlier native call")
                })?;
            target.outputs = Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
        }
    }
    Ok(())
}
