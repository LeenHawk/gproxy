use crate::TransformError;
use bytes::Bytes;

use super::{State, ToolStart};

impl State {
    pub(super) fn complete_function_item(
        &mut self,
        start: ToolStart,
        arguments: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        let source_id = start.source_id.clone();
        let output_index = start.output_index;
        let kind = start.kind;
        let mut output = self.start_tool(start)?;
        output.extend(self.finish_tool(&source_id, output_index, kind, arguments)?);
        Ok(output)
    }
}

pub(super) fn source_id(id: Option<&str>, output_index: u32) -> String {
    id.map(str::to_owned)
        .unwrap_or_else(|| format!("output:{output_index}"))
}
