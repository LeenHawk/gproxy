use super::{
    items::{Item, item_id},
    parts::Part,
    *,
};
use crate::transform::Converted;
impl ResponsesStreamCollector {
    pub(super) fn check_response(
        &self,
        v: &r::GenerateContentResponseBody,
        terminal: bool,
    ) -> Result<(), TransformError> {
        if v.id.is_empty() || v.model.is_empty() || v.created_at < 0 {
            return Err(invalid("missing/invalid native response identity facts"));
        }
        if let Some(old) = &self.response
            && (old.id != v.id || old.model != v.model || old.created_at != v.created_at)
        {
            return Err(invalid("response identity/model/creation changed"));
        }
        if !terminal
            && (!matches!(
                v.status,
                Some(r::ResponseStatus::InProgress | r::ResponseStatus::Queued)
            ) || v.error.is_some()
                || v.incomplete_details.is_some()
                || v.completed_at.flatten().is_some())
        {
            return Err(invalid(
                "nonterminal response contains terminal status/facts",
            ));
        }
        Ok(())
    }
    pub(super) fn terminal(
        &mut self,
        v: r::GenerateContentResponseBody,
        status: r::ResponseStatus,
    ) -> Result<(), TransformError> {
        self.check_response(&v, true)?;
        for item in &v.output {
            super::status::item(item, status == r::ResponseStatus::Completed)?;
        }
        let (text, json) = v
            .output
            .iter()
            .map(item_sizes)
            .fold((0usize, 0usize), |(a, b), (c, d)| {
                (a.saturating_add(c), b.saturating_add(d))
            });
        if text > self.limits.max_text_bytes || json > self.limits.max_json_bytes {
            return Err(limit());
        }

        if v.status != Some(status)
            || v.error.is_some()
            || (status == r::ResponseStatus::Completed && v.incomplete_details.is_some())
        {
            return Err(invalid("terminal response status mismatch"));
        }
        if status == r::ResponseStatus::Incomplete
            && v.incomplete_details
                .as_ref()
                .and_then(|v| v.reason)
                .is_none()
        {
            return Err(invalid("incomplete response missing reason"));
        }
        if status == r::ResponseStatus::Completed
            && self
                .items
                .iter()
                .any(|item| item.parts.values().any(|part| part.incomplete))
        {
            return Err(invalid(
                "completed response contains incomplete summary part",
            ));
        }
        if self.items.len() != v.output.len()
            || self
                .items
                .iter()
                .zip(&v.output)
                .any(|(a, b)| !a.done || &a.value != b)
        {
            return Err(invalid(
                "terminal response contradicts/misses completed output items",
            ));
        }
        if let Some(Some(text)) = &v.output_text {
            let actual: String = v
                .output
                .iter()
                .filter_map(|v| {
                    if let r::ResponseOutputItem::Message(v) = v {
                        Some(v)
                    } else {
                        None
                    }
                })
                .flat_map(|v| &v.content)
                .filter_map(|v| {
                    if let i::OutputContent::Text(v) = v {
                        Some(v.text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if text != &actual {
                return Err(invalid("output_text contradicts output"));
            }
        }
        if let Some(u) = v.usage.as_ref().and_then(Option::as_ref)
            && ([
                u.input_tokens,
                u.output_tokens,
                u.total_tokens,
                u.input_tokens_details.cached_tokens,
                u.input_tokens_details.cache_write_tokens,
                u.output_tokens_details.reasoning_tokens,
            ]
            .iter()
            .any(|v| *v < 0)
                || u.input_tokens.checked_add(u.output_tokens) != Some(u.total_tokens)
                || u.input_tokens_details
                    .cached_tokens
                    .checked_add(u.input_tokens_details.cache_write_tokens)
                    .is_none_or(|total| total > u.input_tokens)
                || u.output_tokens_details.reasoning_tokens > u.output_tokens)
        {
            return Err(invalid("invalid terminal usage"));
        }
        self.response = Some(v);
        self.done = true;
        Ok(())
    }
    pub(super) fn item(&mut self, index: i64, id: &str) -> Result<&mut Item, TransformError> {
        let item = self
            .items
            .get_mut(usize::try_from(index).map_err(|_| invalid("negative output index"))?)
            .ok_or_else(|| invalid("unknown output index"))?;
        if !item.done && item_id(&item.value).is_none() {
            if id.is_empty() || !self.ids.insert(id.to_owned()) {
                return Err(invalid("empty/duplicate late item ID"));
            }
            match &mut item.value {
                r::ResponseOutputItem::FunctionCall(v) => v.id = Some(id.into()),
                r::ResponseOutputItem::CustomToolCall(v) => v.id = Some(id.into()),
                _ => return Err(invalid("item cannot receive late ID")),
            }
        }
        if item.done || item_id(&item.value) != Some(id) {
            return Err(invalid("item ID/index/phase mismatch"));
        }
        Ok(item)
    }
    pub(super) fn part(
        &mut self,
        index: i64,
        id: &str,
        content: i64,
        summary: bool,
    ) -> Result<&mut Part, TransformError> {
        self.item(index, id)?
            .parts
            .get_mut(&(summary, content))
            .ok_or_else(|| invalid("content part not added"))
    }
    pub(super) fn add_part(
        &mut self,
        index: i64,
        id: &str,
        content: i64,
        summary: bool,
        value: s::OutputContentPart,
    ) -> Result<(), TransformError> {
        let text = match &value {
            s::OutputContentPart::Text(v) => &v.text,
            s::OutputContentPart::Refusal(v) => &v.refusal,
            s::OutputContentPart::Reasoning(v) => &v.text,
        };
        self.charge(text.len(), false)?;
        let item = self.item(index, id)?;
        let expected =
            item.base_parts(summary)? + item.parts.keys().filter(|(s, _)| *s == summary).count();
        if content != expected as i64 {
            return Err(invalid("noncontiguous/duplicate content index"));
        }
        if !matches!(
            (&item.value, &value, summary),
            (
                r::ResponseOutputItem::Message(_),
                s::OutputContentPart::Text(_) | s::OutputContentPart::Refusal(_),
                false
            ) | (
                r::ResponseOutputItem::Reasoning(_),
                s::OutputContentPart::Reasoning(_),
                _
            )
        ) {
            return Err(invalid("content type incompatible with item"));
        }
        item.parts
            .insert((summary, content), Part::new(value, summary));
        Ok(())
    }
    pub(super) fn delta(
        &mut self,
        index: i64,
        id: &str,
        content: i64,
        kind: u8,
        text: &str,
    ) -> Result<&mut Part, TransformError> {
        self.charge(text.len(), false)?;
        let p = self.part(index, id, content, kind == 3)?;
        p.check_kind(kind)?;
        p.seeded = false;
        p.text_mut().push_str(text);
        Ok(p)
    }
    pub(super) fn text_done(
        &mut self,
        index: i64,
        id: &str,
        content: i64,
        kind: u8,
        text: &str,
        logs: &[s::StreamLogprob],
    ) -> Result<(), TransformError> {
        let p = self.part(index, id, content, kind == 3)?;
        p.check_kind(kind)?;
        p.finish_text(text, logs)
    }
    pub(super) fn argument(
        &mut self,
        index: i64,
        id: &str,
        kind: u8,
        text: &str,
        done: bool,
    ) -> Result<(), TransformError> {
        if !done {
            self.charge(text.len(), true)?;
        }
        let item = self.item(index, id)?;
        let actual = match &item.value {
            r::ResponseOutputItem::FunctionCall(_) => 0,
            r::ResponseOutputItem::CustomToolCall(_) => 1,
            r::ResponseOutputItem::CodeInterpreterCall(_) => 2,
            r::ResponseOutputItem::McpCall(_) => 3,
            _ => 255,
        };
        if actual != kind || item.argument_done {
            return Err(invalid("argument event type/phase mismatch"));
        }
        item.argument_streamed = true;
        if done {
            if item.arguments()? != text {
                return Err(invalid("argument done contradicts deltas"));
            }
            item.argument_done = true;
        } else {
            item.arguments_mut()?.push_str(text);
        }
        Ok(())
    }
    pub(super) fn charge(&mut self, n: usize, json: bool) -> Result<(), TransformError> {
        let (used, cap) = if json {
            (&mut self.json_bytes, self.limits.max_json_bytes)
        } else {
            (&mut self.text_bytes, self.limits.max_text_bytes)
        };
        *used = used.checked_add(n).ok_or_else(limit)?;
        if *used > cap { Err(limit()) } else { Ok(()) }
    }
    pub(super) fn charge_item(
        &mut self,
        item: &r::ResponseOutputItem,
    ) -> Result<(), TransformError> {
        let (text, json) = item_sizes(item);
        if text > self.limits.max_text_bytes || json > self.limits.max_json_bytes {
            Err(limit())
        } else {
            Ok(())
        }
    }

    pub(super) fn check_retained_sizes(&self) -> Result<(), TransformError> {
        let (mut text, mut json) = (0usize, 0usize);
        for item in &self.items {
            let (t, j) = item_sizes(&item.value);
            text = text.saturating_add(t);
            json = json.saturating_add(j);
            if !item.done {
                for part in item.parts.values() {
                    text = text.saturating_add(part.text().len());
                }
            }
        }
        if text > self.limits.max_text_bytes || json > self.limits.max_json_bytes {
            Err(limit())
        } else {
            Ok(())
        }
    }
    pub fn finish(self) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.done {
            return Err(invalid("stream failed or ended before terminal response"));
        }
        Ok(Converted {
            value: self.response.ok_or_else(|| invalid("missing response"))?,
            report: self.report,
        })
    }
}

pub(super) fn item_sizes(item: &r::ResponseOutputItem) -> (usize, usize) {
    let (mut text, mut json) = (0usize, 0usize);
    match item {
        r::ResponseOutputItem::Message(v) => {
            for p in &v.content {
                text = text.saturating_add(match p {
                    i::OutputContent::Text(v) => v.text.len(),
                    i::OutputContent::Refusal(v) => v.refusal.len(),
                });
            }
        }
        r::ResponseOutputItem::Reasoning(v) => {
            for p in &v.summary {
                text = text.saturating_add(p.text.len());
            }
            for p in v.content.iter().flatten() {
                text = text.saturating_add(p.text.len());
            }
        }
        r::ResponseOutputItem::FunctionCall(v) => json = v.arguments.len(),
        r::ResponseOutputItem::CustomToolCall(v) => json = v.input.len(),
        r::ResponseOutputItem::McpCall(v) => json = v.arguments.len(),
        r::ResponseOutputItem::CodeInterpreterCall(v) => {
            json = v.code.as_ref().map_or(0, String::len)
        }
        _ => {}
    }
    (text, json)
}
