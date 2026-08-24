use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::generate_content::responses::{
    KnownResponseStreamEvent as Known, ResponseStreamEvent,
};

use super::ToolAliases;
use super::alias::{Alias, AliasKind, alias, canonical, canonical_existing};
use crate::codex::sse::event;

impl ToolAliases {
    pub(in crate::codex::sse) fn normalize(
        &mut self,
        mut event: ResponseStreamEvent,
    ) -> Result<Vec<ResponseStreamEvent>, ChannelError> {
        let ResponseStreamEvent::Known(known) = &mut event else {
            return Ok(vec![event]);
        };
        match known.as_mut() {
            Known::ResponseOutputItemAdded(payload) => {
                let Some(alias) = alias(&payload.item) else {
                    return Ok(vec![event]);
                };
                self.aliases.insert(payload.output_index, alias);
                Ok(Vec::new())
            }
            Known::ResponseFunctionCallArgumentsDelta(payload) => {
                let Some(alias) = self.matching(payload.output_index, AliasKind::Shell)? else {
                    return Ok(vec![event]);
                };
                alias.input.push_str(&payload.delta);
                Ok(Vec::new())
            }
            Known::ResponseCustomToolCallInputDelta(payload) => {
                let Some(alias) = self.matching(payload.output_index, AliasKind::ApplyPatch)?
                else {
                    return Ok(vec![event]);
                };
                alias.input.push_str(&payload.delta);
                Ok(Vec::new())
            }
            Known::ResponseFunctionCallArgumentsDone(payload) => {
                let Some(alias) = self.matching(payload.output_index, AliasKind::Shell)? else {
                    return Ok(vec![event]);
                };
                payload.arguments.clone_into(&mut alias.input);
                let item = canonical(alias.kind, alias.id.clone(), &alias.call_id, &alias.input)?;
                alias.item = Some(item.clone());
                Ok(vec![event::output_item_added(
                    payload.output_index,
                    item,
                    payload.sequence_number.take(),
                    std::mem::take(&mut payload.rest),
                )])
            }
            Known::ResponseCustomToolCallInputDone(payload) => {
                let Some(alias) = self.matching(payload.output_index, AliasKind::ApplyPatch)?
                else {
                    return Ok(vec![event]);
                };
                payload.input.clone_into(&mut alias.input);
                let item = canonical(alias.kind, alias.id.clone(), &alias.call_id, &alias.input)?;
                alias.item = Some(item.clone());
                Ok(vec![event::output_item_added(
                    payload.output_index,
                    item,
                    payload.sequence_number.take(),
                    std::mem::take(&mut payload.rest),
                )])
            }
            Known::ResponseOutputItemDone(payload) => {
                let item = if let Some(alias) = self.aliases.get(&payload.output_index) {
                    match alias.item.clone() {
                        Some(item) => Some(item),
                        None => canonical_existing(&payload.item)?,
                    }
                } else {
                    canonical_existing(&payload.item)?
                };
                if let Some(item) = item {
                    *payload.item = item;
                }
                Ok(vec![event])
            }
            Known::ResponseCompleted(payload)
            | Known::ResponseIncomplete(payload)
            | Known::ResponseFailed(payload) => {
                for (index, item) in payload.response.output.iter_mut().enumerate() {
                    let remembered = u32::try_from(index)
                        .ok()
                        .and_then(|index| self.aliases.get(&index))
                        .and_then(|alias| alias.item.clone());
                    let canonical = match remembered {
                        Some(item) => Some(item),
                        None => canonical_existing(item)?,
                    };
                    if let Some(canonical) = canonical {
                        *item = canonical;
                    }
                }
                Ok(vec![event])
            }
            Known::ResponseCreated(_)
            | Known::ResponseInProgress(_)
            | Known::ResponseQueued(_)
            | Known::ResponseContentPartAdded(_)
            | Known::ResponseContentPartDone(_)
            | Known::ResponseOutputTextDelta(_)
            | Known::ResponseOutputTextDone(_)
            | Known::ResponseOutputTextAnnotationAdded(_)
            | Known::ResponseRefusalDelta(_)
            | Known::ResponseRefusalDone(_)
            | Known::ResponseReasoningSummaryPartAdded(_)
            | Known::ResponseReasoningSummaryPartDone(_)
            | Known::ResponseReasoningSummaryTextDelta(_)
            | Known::ResponseReasoningSummaryTextDone(_)
            | Known::ResponseReasoningTextDelta(_)
            | Known::ResponseReasoningTextDone(_)
            | Known::ResponseAudioDelta(_)
            | Known::ResponseAudioDone(_)
            | Known::ResponseAudioTranscriptDelta(_)
            | Known::ResponseAudioTranscriptDone(_)
            | Known::ResponseImageGenerationCallCompleted(_)
            | Known::ResponseImageGenerationCallGenerating(_)
            | Known::ResponseImageGenerationCallInProgress(_)
            | Known::ResponseImageGenerationCallPartialImage(_)
            | Known::ResponseFileSearchCallInProgress(_)
            | Known::ResponseFileSearchCallSearching(_)
            | Known::ResponseFileSearchCallCompleted(_)
            | Known::ResponseWebSearchCallInProgress(_)
            | Known::ResponseWebSearchCallSearching(_)
            | Known::ResponseWebSearchCallCompleted(_)
            | Known::ResponseCodeInterpreterCallInProgress(_)
            | Known::ResponseCodeInterpreterCallInterpreting(_)
            | Known::ResponseCodeInterpreterCallCompleted(_)
            | Known::ResponseCodeInterpreterCallCodeDelta(_)
            | Known::ResponseCodeInterpreterCallCodeDone(_)
            | Known::ResponseMcpCallArgumentsDelta(_)
            | Known::ResponseMcpCallArgumentsDone(_)
            | Known::ResponseMcpCallInProgress(_)
            | Known::ResponseMcpCallCompleted(_)
            | Known::ResponseMcpCallFailed(_)
            | Known::ResponseMcpListToolsInProgress(_)
            | Known::ResponseMcpListToolsCompleted(_)
            | Known::ResponseMcpListToolsFailed(_)
            | Known::Error(_) => Ok(vec![event]),
        }
    }

    fn matching(
        &mut self,
        output_index: u32,
        expected: AliasKind,
    ) -> Result<Option<&mut Alias>, ChannelError> {
        let Some(alias) = self.aliases.get_mut(&output_index) else {
            return Ok(None);
        };
        if alias.kind != expected {
            return Err(ChannelError::Decode(
                "Codex tool alias event kind does not match its item".into(),
            ));
        }
        Ok(Some(alias))
    }
}
