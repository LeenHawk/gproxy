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
    event::NativeEvent, invoke::ClientFull, output::Encoder,
};
use super::{codec_error, conflict, invalid, limit};
use crate::{
    adapt::generate::{
        GenerationStateAccess,
        fanout::{FanoutOptions, group_id},
    },
    capability::StateStore,
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole},
    },
};
use std::collections::{BTreeMap, BTreeSet};

pub use bridge::FanoutBridge;
pub use event::FanoutEvent;

/// Caller-owned progress retains every child receipt and the active stream
/// across cancellation, in memory only. Dropping it cancels the active body.
pub struct FanoutStream<B: FanoutBridge>
where
    B::ClientEvent: FanoutEvent,
{
    children: Vec<StreamInvocation<B>>,
    id: String,
    settings: StreamSettings,
    index: usize,
    seeded: bool,
    seen: BTreeMap<String, usize>,
    response_ids: BTreeSet<String>,
    completed: Vec<ClientFull<B>>,
    encoder: Encoder,
    collector: Option<<B::ClientEvent as NativeEvent>::Collector>,
    created: Option<i64>,
    report: Report,
    aggregate: Option<ClientFull<B>>,
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
        for child in &children {
            child.binding.check(state)?;
        }
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
            index: 0,
            seeded: false,
            seen: Default::default(),
            response_ids,
            completed: Vec::new(),
            encoder: Encoder::new(settings.client_framing, settings.codec),
            collector: Some(collector),
            created: None,
            report: Report::default(),
            aggregate: None,
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
        self.aggregate.as_ref()
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
}
