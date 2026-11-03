use super::*;
use RealtimeEventKind as K;
use std::collections::{BTreeMap, BTreeSet};

const SPEECH: u8 = 0;
const TEXT: u8 = 1;
const TOOL: u8 = 2;
const TOOL_RESULT: u8 = 3;
/// Incremental persistence collector. Audio bytes and raw event envelopes never
/// enter its state, including when native envelopes contain several payloads.
#[derive(Default)]
pub struct RealtimeCollector {
    entries: Vec<Option<Message>>,
    indices: BTreeMap<(RealtimeItem, u8), usize>,
    interrupted: BTreeMap<RealtimeItem, Option<u64>>,
    assistant: BTreeSet<usize>,
}
impl RealtimeCollector {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, event: &RealtimeEvent) {
        let vendor = &event.metadata.vendor;
        match &event.kind {
            K::InputCommitted { item } | K::SpeechStarted { item, .. } => {
                self.index(item, SPEECH);
            }
            K::InputTranscriptDelta { item, delta } => {
                self.text(vendor, item, SPEECH, Role::User, delta, false)
            }
            K::InputTranscriptDone { item, text } => {
                self.text(vendor, item, SPEECH, Role::User, text, true)
            }
            K::InputTranscriptionFailed { item, .. } => {
                if let Some(index) = self.indices.get(&(item.clone(), SPEECH)) {
                    self.entries[*index] = None;
                }
            }
            K::OutputTranscriptDelta { item, delta } => {
                self.text(vendor, item, SPEECH, Role::Assistant, delta, false)
            }
            K::OutputTranscriptDone { item, text } => {
                self.text(vendor, item, SPEECH, Role::Assistant, text, true)
            }
            K::TextDelta { item, delta } => {
                self.text(vendor, item, TEXT, Role::Assistant, delta, false)
            }
            K::TextDone { item, text } => {
                self.text(vendor, item, TEXT, Role::Assistant, text, true)
            }
            K::InputText { item, text } => self.text(vendor, item, TEXT, Role::User, text, true),
            K::ToolCall { item, call } => {
                let index = self.index(item, TOOL);
                let mut block = Block::new(Content::ToolCall(call.clone()));
                set_meta(&mut block, vendor, item, false);
                self.entries[index] = Some(Message {
                    role: Role::Assistant,
                    content: vec![block],
                });
                self.assistant.insert(index);
            }
            K::ToolResultSent { result } => {
                let item = RealtimeItem::new(result.id.clone(), 0);
                let index = self.index(&item, TOOL_RESULT);
                let mut block = Block::new(Content::ToolResult(result.clone()));
                set_meta(&mut block, vendor, &item, false);
                self.entries[index] = Some(Message {
                    role: Role::Tool,
                    content: vec![block],
                });
            }
            K::Interrupted { item, played_ms } => self.interrupt(item, *played_ms),
            K::ResponseDone { finish_reason, .. } => {
                if finish_reason.unified == FinishReasonKind::Interrupted {
                    self.interrupt_pending();
                }
                self.assistant.clear();
            }
            K::Closed { .. } => {
                self.interrupt_pending();
                self.assistant.clear();
            }
            _ => {}
        }
    }
    pub fn into_conversation(self) -> Conversation {
        Conversation::new(self.entries.into_iter().flatten().collect())
    }
    fn index(&mut self, item: &RealtimeItem, kind: u8) -> usize {
        *self.indices.entry((item.clone(), kind)).or_insert_with(|| {
            let index = self.entries.len();
            self.entries.push(None);
            index
        })
    }
    fn interrupt_pending(&mut self) {
        let items = self
            .indices
            .iter()
            .filter_map(|((item, kind), index)| {
                (*kind == SPEECH && self.assistant.contains(index)).then_some(item.clone())
            })
            .collect::<Vec<_>>();
        for item in items {
            self.interrupt(&item, None);
        }
    }
    fn text(
        &mut self,
        vendor: &Vendor,
        item: &RealtimeItem,
        kind: u8,
        role: Role,
        value: &str,
        replace: bool,
    ) {
        if kind == SPEECH && self.interrupted.contains_key(item) {
            return;
        }
        let index = self.index(item, kind);
        let message = self.entries[index].get_or_insert_with(|| {
            let mut block = Block::text("");
            set_meta(&mut block, vendor, item, kind == SPEECH);
            Message {
                role: role.clone(),
                content: vec![block],
            }
        });
        if let Some(Block {
            content: Content::Text(text),
            ..
        }) = message.content.first_mut()
        {
            if replace {
                text.text = value.to_owned();
            } else {
                text.text.push_str(value);
            }
        }
        if role == Role::Assistant {
            self.assistant.insert(index);
        }
    }
    fn interrupt(&mut self, item: &RealtimeItem, played_ms: Option<u64>) {
        let recorded = self.interrupted.entry(item.clone()).or_insert(played_ms);
        if played_ms.is_some() {
            *recorded = played_ms;
        }
        let played_ms = *recorded;
        let index = self.index(item, SPEECH);
        let block = Block::new(Content::Interrupted(Interruption { played_ms }));
        self.entries[index] = Some(Message {
            role: Role::Assistant,
            content: vec![block],
        });
    }
}
/// SDK-owned per-block realtime metadata, stored under the session vendor.
/// Default values are skipped so the archive only carries what matters.
#[derive(Serialize)]
struct BlockMetadata<'a> {
    realtime_item_id: &'a str,
    #[serde(skip_serializing_if = "is_zero")]
    realtime_content_index: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    realtime_speech_transcript: bool,
}
fn is_zero(value: &i64) -> bool {
    *value == 0
}
fn set_meta(block: &mut Block, vendor: &Vendor, item: &RealtimeItem, speech: bool) {
    let meta = BlockMetadata {
        realtime_item_id: &item.id,
        realtime_content_index: item.content_index,
        realtime_speech_transcript: speech,
    };
    let Ok(Value::Object(fields)) = serde_json::to_value(meta) else {
        unreachable!("a struct serializes to an object");
    };
    block
        .provider_metadata
        .entry(vendor.clone())
        .or_default()
        .extend(fields);
}
