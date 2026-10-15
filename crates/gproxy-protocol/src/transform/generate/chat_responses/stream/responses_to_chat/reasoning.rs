use super::*;

pub(in super::super) struct ReasoningState {
    pub summary: BTreeMap<i64, TextPart>,
    pub cursor: i64,
    pub final_item: Option<i::ReasoningItem>,
}

impl ResponsesToChatStream {
    pub(super) fn reasoning_mut(
        &mut self,
        index: i64,
        id: &str,
    ) -> Result<Option<&mut ReasoningState>, TransformError> {
        self.bind_item(index, Some(id))?;
        match self.items.get_mut(&index).map(|item| &mut item.kind) {
            Some(ItemKind::Excluded) => Ok(None),
            Some(ItemKind::Reasoning(state)) => Ok(Some(state)),
            _ => Err(invalid("reasoning event without reasoning item")),
        }
    }

    fn emit_reasoning(
        &mut self,
        text: String,
        new_part: bool,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        let text = if new_part && self.reasoning_parts > 0 {
            format!("\n{text}")
        } else {
            text
        };
        if new_part {
            self.reasoning_parts += 1;
        }
        self.reasoning_text.push_str(&text);
        let mut delta = cs::Delta::builder().build();
        delta.reasoning_content = Some(Some(text));
        self.emit_delta(delta, out)
    }

    pub(super) fn flush_reasoning(
        &mut self,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        while let Some(mut item) = self.items.remove(&self.reasoning_cursor) {
            let mut advance = true;
            if let ItemKind::Reasoning(state) = &mut item.kind {
                while let Some(part) = state.summary.get_mut(&state.cursor) {
                    if !part.emitted || !part.pending.is_empty() {
                        self.emit_reasoning(std::mem::take(&mut part.pending), !part.emitted, out)?;
                        part.emitted = true;
                    }
                    if !part.done {
                        break;
                    }
                    state.cursor += 1;
                }
                if let Some(final_item) = state.final_item.take() {
                    for part in final_item.content.iter().flatten() {
                        self.emit_reasoning(part.text.clone(), true, out)?;
                    }
                    // Encrypted data and carried signatures require the final
                    // item. Emit each envelope once, without replaying summaries.
                    let details = crate::transform::generate::reasoning_details::from_responses(
                        &final_item,
                        self.reasoning_details as i64,
                    );
                    if !details.is_empty() {
                        self.reasoning_details += details.len();
                        let mut delta = cs::Delta::builder().build();
                        delta.reasoning_details = Some(Some(details));
                        self.emit_delta(delta, out)?;
                    }
                } else {
                    advance = false;
                }
            }
            self.items.insert(self.reasoning_cursor, item);
            if !advance {
                break;
            }
            self.reasoning_cursor += 1;
        }
        Ok(())
    }
}
