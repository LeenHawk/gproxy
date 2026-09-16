//! Ordered incremental multi-candidate calls. Child streams are polled one at a
//! time; their first content is exposed before EOF. No started POST is replayed.
mod bridge;
mod driver;
mod event;
mod images;
mod prepare;
mod run;
use super::{
    StreamChunk, StreamInvocation, StreamSettings, StreamStart, bridge::StreamBridge,
    event::NativeEvent, invoke::ClientFull, output::Encoder, reservation::Reservation,
};
use super::{codec_error, conflict, invalid, limit};
use crate::{
    adapt::generate::{
        GenerationProgress, GenerationStateAccess,
        fanout::{FanoutOptions, group_id},
    },
    capability::StateStore,
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, IdentityStateRecord},
    },
};
pub use bridge::FanoutBridge;
pub use event::FanoutEvent;
use std::collections::{BTreeMap, BTreeSet};
/// Caller-owned progress retains every child receipt, pending journal write and
/// active stream across cancellation. Dropping it cancels the active body.
pub struct FanoutStream<B: FanoutBridge>
where
    B::ClientEvent: FanoutEvent,
{
    children: Vec<StreamInvocation<B>>,
    id: String,
    settings: StreamSettings,
    manifest: Reservation,
    group: GenerationProgress<()>,
    group_saved: bool,
    index: usize,
    seeded: bool,
    fixed: BTreeSet<String>,
    seen: BTreeMap<String, usize>,
    response_ids: BTreeSet<String>,
    child_record: Option<Reservation>,
    completed: Vec<ClientFull<B>>,
    encoder: Encoder,
    collector: Option<<B::ClientEvent as NativeEvent>::Collector>,
    created: Option<i64>,
    report: Report,
    terminal_record: Option<Reservation>,
    aggregate: Option<ClientFull<B>>,
    aggregate_saved: bool,
    tail: Option<B::ClientEvent>,
    tail_emitted: bool,
    emit_usage: bool,
    finished: bool,
    failed: bool,
    events: usize,
    retained_bytes: usize,
}
impl<B: FanoutBridge> FanoutStream<B>
where
    B::ClientEvent: FanoutEvent,
{
    async fn new<S: StateStore>(
        mut children: Vec<StreamInvocation<B>>,
        options: FanoutOptions,
        original: Vec<u8>,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let ids: Vec<_> = children
            .iter()
            .map(|v| v.selected.identities.clone())
            .collect();
        let id = group_id(&options)?;
        let settings = children[0].settings;
        if children.len() > settings.events.max_choices.min(options.max_children)
            || children
                .len()
                .checked_add(1)
                .is_none_or(|n| n > state.max_records)
        {
            return Err(limit("fanout count exceeds event/state budget"));
        }
        let mut fixed = BTreeSet::new();
        let mut entries = Vec::new();
        for child in &children {
            child.preparation.verify(state).await?;
            fixed.extend(
                child
                    .bridge
                    .as_ref()
                    .ok_or_else(|| invalid("child bridge consumed"))?
                    .fixed_ids(),
            );
            let request = crate::codec::encode_json(child.target_request(), settings.codec)
                .map_err(codec_error)?;
            entries.push(serde_json::json!({"request":std::str::from_utf8(&request).map_err(|e|invalid(e.to_string()))?,"request_namespace":child.selected.identities.request.namespace(),"response_namespace":child.selected.identities.response.namespace(),"request_policy":child.selected.identities.request_policy,"response_policy":child.selected.identities.response_policy,"path":child.selected.endpoint.path,"query":child.selected.endpoint.query,"headers":child.selected.endpoint.headers.iter().map(|(k,v)|(k.as_str(),v.as_bytes())).collect::<Vec<_>>()}));
        }
        if fixed.len() > state.max_records {
            return Err(limit("fanout fixed IDs exceed state budget"));
        }
        let payload=crate::codec::encode_json(&serde_json::json!({"schema":1,"id":id,"original":std::str::from_utf8(&original).map_err(|e|invalid(e.to_string()))?,"children":entries,"fixed_ids":fixed,"target":state.target,"conversation":state.conversation_key,"expires_at":state.expires_at}),settings.codec).map_err(codec_error)?.to_vec();
        let mut manifest = Reservation::record(
            format!(
                "fanout-stream:{}:{}:{}",
                state.conversation_key.len(),
                state.conversation_key,
                id
            ),
            payload,
            state,
        )?;
        manifest.reserve(state).await?;
        let collector = B::ClientEvent::collector(
            IdentityFlow::new(options.namespace),
            ids[0].response_policy.clone(),
            settings.events,
        );
        let emit_usage = children[0].emit_usage;
        // Child collection always retains measured usage. Only the aggregate's
        // final wire chunk follows the actual client's include_usage option.
        for child in &mut children {
            child.emit_usage = true;
        }
        let response_ids = BTreeSet::from([id.clone()]);
        Ok(Self {
            children,
            id,
            settings,
            manifest,
            group: Default::default(),
            group_saved: false,
            index: 0,
            seeded: false,
            fixed,
            seen: Default::default(),
            response_ids,
            child_record: None,
            completed: Vec::new(),
            encoder: Encoder::new(settings.client_framing, settings.codec),
            collector: Some(collector),
            created: None,
            report: Report::default(),
            terminal_record: None,
            aggregate: None,
            aggregate_saved: false,
            tail: None,
            tail_emitted: false,
            emit_usage,
            finished: false,
            failed: false,
            events: 0,
            retained_bytes: 0,
        })
    }
    pub fn children(&self) -> &[StreamInvocation<B>] {
        &self.children
    }
    pub fn response_id(&self) -> &str {
        &self.id
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn client_result(&self) -> Option<&ClientFull<B>> {
        self.aggregate_saved
            .then_some(self.aggregate.as_ref())
            .flatten()
    }
    fn fail(&mut self) {
        self.failed = true;
        for child in &mut self.children {
            child.reader = None;
        }
    }
    fn emit(
        &mut self,
        event: B::ClientEvent,
    ) -> Result<StreamChunk<B::ClientEvent>, TransformError> {
        self.events = self
            .events
            .checked_add(1)
            .ok_or_else(|| limit("fanout event overflow"))?;
        if self.events > self.settings.events.max_events {
            return Err(limit("fanout event budget exceeded"));
        }
        let bytes = self.encoder.event(&event, self.settings.codec)?;
        Ok(StreamChunk {
            event: Some(event),
            bytes,
            finished: false,
        })
    }
    fn receipt<S: StateStore>(
        &self,
        stage: &str,
        value: &impl serde::Serialize,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Reservation, TransformError> {
        let payload=crate::codec::encode_json(&serde_json::json!({"schema":1,"id":self.id,"target":state.target,"conversation":state.conversation_key,"expires_at":state.expires_at,"value":value}),self.settings.codec).map_err(codec_error)?.to_vec();
        Reservation::record(
            format!(
                "fanout-stream-{}:{}:{}:{}",
                stage,
                state.conversation_key.len(),
                state.conversation_key,
                self.id
            ),
            payload,
            state,
        )
    }
}
