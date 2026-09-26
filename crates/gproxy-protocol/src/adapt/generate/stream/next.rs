use super::super::GenerationStateAccess;
use super::{
    StreamChunk, StreamInvocation, bridge::StreamBridge, event::NativeEvent, invoke::ReadyChunk,
    ledger::ToolDeclaration, reader::NativeFrame,
};
use crate::{
    capability::StateStore,
    codec,
    transform::{Report, TransformError},
};

impl<B: StreamBridge> StreamInvocation<B> {
    /// Collect the mapped stream into its concrete complete client wire response.
    /// The host calls `start` once first. This does not perform another POST.
    pub async fn collect<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<super::super::GenerationOutcome<super::invoke::ClientFull<B>>, TransformError> {
        self.binding.check(state)?;
        if let Some(raw) = &self.rejected {
            return Ok(super::super::GenerationOutcome::Rejected(
                crate::WireResponse {
                    status: raw.status,
                    headers: raw.headers.clone(),
                    body: raw.body.clone(),
                },
            ));
        }
        while self.next(state).await?.is_some() {}
        let body = self
            .client_result()
            .ok_or_else(|| super::missing("client final response is not durably committed"))?
            .clone();
        codec::encode_json(&body, self.settings.codec).map_err(super::codec_error)?;
        let metadata = self
            .metadata
            .as_ref()
            .ok_or_else(|| super::missing("native HTTP metadata unavailable"))?;
        Ok(super::super::GenerationOutcome::Success {
            response: crate::WireResponse {
                status: metadata.status,
                headers: super::output::headers(
                    metadata.headers.clone(),
                    super::reader::SourceFraming::JsonArray,
                ),
                body,
            },
            report: self.report.clone(),
        })
    }

    /// Returns the next encoded client event only after required identity CAS
    /// writes have durable acknowledgments. Cancellation retains that event.
    pub async fn next<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        if self.websocket_terminal {
            self.next_inner(
                state,
                Some(&mut futures_util::stream::poll_fn(|_| {
                    std::task::Poll::Ready(None)
                })),
            )
            .await
        } else {
            self.next_inner(state, None).await
        }
    }
    pub(super) async fn next_inner<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
        external: Option<&mut super::resource_map::ExternalSource<'_, B::NativeEvent>>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        self.next_mapped(state, external, None).await
    }
    pub(super) async fn next_mapped<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
        mut external: Option<&mut super::resource_map::ExternalSource<'_, B::NativeEvent>>,
        mut mapping: Option<&mut dyn super::resource_map::ResourceMapping<B, S>>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        use futures_util::StreamExt;
        if self.image_resources_required && mapping.is_none() {
            return Err(super::missing(
                "continue this invocation with its image resource progress",
            ));
        }

        self.binding.check(state)?;
        if mapping
            .as_ref()
            .is_some_and(|mapper| mapper.revision() != self.resource_revision)
        {
            return Err(super::conflict(
                "image resource progress changed during invocation",
            ));
        }
        if self.failed {
            return Err(super::invalid("stream invocation failed"));
        }
        if self.finished {
            return Ok(None);
        }
        if self.reader.is_none() && external.is_none() {
            return Err(super::missing("successful stream start required"));
        }
        loop {
            if let Some(original) = self.pending_native.as_ref() {
                let view = if let Some(mapper) = mapping.as_deref_mut() {
                    self.resource_revision = mapper.begin_step()?;
                    let value = match mapper.source(original).await {
                        Ok(value) => value,
                        Err(error) => {
                            if mapper.failed() {
                                self.failed = true;
                                self.reader = None;
                            }
                            return Err(self.image_resource_error(error));
                        }
                    };
                    self.resource_revision = mapper.revision();
                    value
                } else {
                    original.clone()
                };
                let original = self.pending_native.take().expect("retained native event");
                let terminal = external.is_some() && original.is_terminal();
                if let Err(error) = self.source_event(original, view) {
                    self.failed = true;
                    self.reader = None;
                    return Err(error);
                }
                if terminal {
                    self.websocket_terminal = true;
                }
            }
            if let Some(events) = self.pending_converted.as_ref() {
                let mapped = if let Some(mapper) = mapping.as_deref_mut() {
                    self.resource_revision = mapper.begin_step()?;
                    let value = match mapper.client(events).await {
                        Ok(value) => value,
                        Err(error) => {
                            if mapper.failed() {
                                self.failed = true;
                                self.reader = None;
                            }
                            return Err(self.image_resource_error(error));
                        }
                    };
                    self.resource_revision = mapper.revision();
                    value
                } else {
                    events.clone()
                };
                self.pending_converted = None;
                if let Err(error) = self.enqueue(mapped) {
                    self.failed = true;
                    self.reader = None;
                    return Err(error);
                }
            }
            if self.finishing_source {
                if let Err(error) = self.finish_client() {
                    self.failed = true;
                    self.reader = None;
                    return Err(error);
                }
                self.finishing_source = false;
            }
            if self.eof && !self.final_saved {
                self.ledger
                    .save(
                        &self.flow,
                        state,
                        (B::NativeEvent::DIALECT == crate::Dialect::OpenAi).then_some(&self.signed),
                    )
                    .await?;
                B::save_final(
                    state,
                    self.native_final
                        .as_ref()
                        .ok_or_else(|| super::invalid("missing collected native response"))?,
                    self.client_final
                        .as_ref()
                        .ok_or_else(|| super::invalid("missing collected client response"))?,
                    &self.flow,
                    &self.signed,
                    &mut self.final_progress,
                )
                .await?;
                if let Some(mapper) = mapping.as_deref_mut() {
                    self.resource_revision = mapper.begin_step()?;
                    mapper
                        .save(
                            self.native_final.as_ref().expect("native final"),
                            self.client_final.as_ref().expect("client final"),
                            &self.flow,
                            state,
                        )
                        .await?;
                    self.resource_revision = mapper.revision();
                }
                if let Some(history) = &mut self.history {
                    let response = B::ClientEvent::responses_history(
                        self.client_final.as_ref().expect("final response checked"),
                    )
                    .ok_or_else(|| {
                        super::invalid("Responses history attached to another dialect")
                    })?;
                    history.save(response, state, self.settings.codec).await?;
                }
                self.final_saved = true;
            }
            if self.ready.is_some() {
                self.ledger
                    .save(
                        &self.flow,
                        state,
                        (B::NativeEvent::DIALECT == crate::Dialect::OpenAi).then_some(&self.signed),
                    )
                    .await?;
                let ready = self.ready.take().expect("pending encoded event retained");
                self.finished = ready.chunk.finished;
                return Ok(Some(ready.chunk));
            }
            if let Some((event, size)) = self.queued.pop_front() {
                if !self.eof && (self.holding || event.is_terminal()) {
                    self.holding = true;
                    self.held.push((event, size));
                    continue;
                }
                self.queued_bytes = self.queued_bytes.saturating_sub(size);
                if let Err(error) = self.prepare_event(event) {
                    self.failed = true;
                    self.reader = None;
                    return Err(error);
                }
                continue;
            }
            if self.eof {
                let bytes = match self.encoder.finish::<B::ClientEvent>() {
                    Ok(value) => value,
                    Err(error) => {
                        self.failed = true;
                        self.reader = None;
                        return Err(error);
                    }
                };
                self.ready = Some(ReadyChunk {
                    chunk: StreamChunk {
                        event: None,
                        bytes,
                        finished: true,
                    },
                });
                continue;
            }
            // Persist late native ID associations even when this source event
            // emitted no new client content, before awaiting another read.
            self.ledger
                .save(
                    &self.flow,
                    state,
                    (B::NativeEvent::DIALECT == crate::Dialect::OpenAi).then_some(&self.signed),
                )
                .await?;
            let next = if let Some(source) = external.as_mut() {
                source
                    .next()
                    .await
                    .transpose()
                    .map(|event| event.map(|value| NativeFrame::Event { name: None, value }))
            } else {
                self.reader
                    .as_mut()
                    .expect("successful start")
                    .next::<B::NativeEvent>()
                    .await
                    .map_err(super::codec_error)
            };
            let result = match next {
                Err(error) => Err(error),
                Ok(Some(NativeFrame::Done)) => self.source_done(),
                Ok(Some(NativeFrame::Event { value, .. })) => {
                    self.last_native_event = Some(value.clone());
                    self.pending_native = Some(value);
                    Ok(())
                }
                Ok(None) => self.finish_source(),
            };
            if let Err(error) = result {
                self.failed = true;
                self.reader = None;
                return Err(error);
            }
        }
    }
    fn image_resource_error(&mut self, error: TransformError) -> TransformError {
        use crate::transform::TransformErrorKind;
        if matches!(
            error.kind(),
            TransformErrorKind::InvalidInput
                | TransformErrorKind::InvalidResult
                | TransformErrorKind::Unsupported
                | TransformErrorKind::Limit
        ) {
            self.failed = true;
            self.reader = None;
        }
        error
    }
    fn source_event(
        &mut self,
        original: B::NativeEvent,
        event: B::NativeEvent,
    ) -> Result<(), TransformError> {
        self.last_native_event = Some(original.clone());
        B::NativeEvent::collect(
            self.source
                .as_mut()
                .ok_or_else(|| super::invalid("native collector consumed"))?,
            original,
        )?;
        let bridge = self
            .bridge
            .as_mut()
            .ok_or_else(|| super::invalid("pair stream consumed"))?;
        let converted = bridge.push(event)?;
        self.flow = bridge.identities().clone();
        if let Some(proof) = bridge.signed_tool_bindings() {
            self.signed = proof.clone();
        }
        self.append_report(converted.report)?;
        self.pending_converted = Some(converted.value);
        Ok(())
    }
    fn source_done(&mut self) -> Result<(), TransformError> {
        if !B::NativeEvent::DONE {
            return Err(super::invalid(
                "DONE is not a terminator for this source dialect",
            ));
        }
        B::NativeEvent::collect_done(
            self.source
                .as_mut()
                .ok_or_else(|| super::invalid("native collector consumed"))?,
        )?;
        self.bridge
            .as_mut()
            .ok_or_else(|| super::invalid("pair stream consumed"))?
            .push_done()
    }
    fn enqueue(&mut self, events: Vec<B::ClientEvent>) -> Result<(), TransformError> {
        for event in events {
            if !self.emit_usage && event.is_usage_only() {
                continue;
            }
            let bytes =
                codec::encode_json(&event, self.settings.codec).map_err(super::codec_error)?;
            let size = bytes.len() as u64;
            self.queued_bytes = self
                .queued_bytes
                .checked_add(size)
                .ok_or_else(|| super::limit("queued client event size overflow"))?;
            if self.queued_bytes > self.settings.codec.max_buffer_bytes
                || self.queued_bytes > self.settings.events.max_pending_bytes as u64
                || self.queued.len().saturating_add(self.held.len())
                    >= self.settings.events.max_events
            {
                return Err(super::limit("pending client events exceed stream budget"));
            }
            self.queued.push_back((event, size));
        }
        Ok(())
    }
    fn observe(&mut self, event: &B::ClientEvent) -> Result<(), TransformError> {
        self.ledger.observe_tools(
            event
                .tool_declarations(B::NativeEvent::DIALECT != crate::Dialect::OpenAiChat)
                .into_iter()
                .map(|(id, kind, name)| ToolDeclaration { id, kind, name })
                .collect(),
        )
    }
    fn prepare_event(&mut self, event: B::ClientEvent) -> Result<(), TransformError> {
        self.observe(&event)?;
        if !self.eof {
            B::ClientEvent::collect(
                self.client
                    .as_mut()
                    .ok_or_else(|| super::invalid("client collector consumed"))?,
                event.clone(),
            )?;
        }
        let bytes = self.encoder.event(&event, self.settings.codec)?;
        self.ready = Some(ReadyChunk {
            chunk: StreamChunk {
                event: Some(event),
                bytes,
                finished: false,
            },
        });
        Ok(())
    }
    fn finish_source(&mut self) -> Result<(), TransformError> {
        let native = B::NativeEvent::collected(
            self.source
                .take()
                .ok_or_else(|| super::invalid("native collector consumed"))?,
        )?;
        self.native_final = Some(native.value);
        self.append_report(native.report)?;
        let end = self
            .bridge
            .take()
            .ok_or_else(|| super::invalid("pair stream consumed"))?
            .finish()?;
        self.flow = end.identities;
        self.signed = end.signed_tool_bindings;
        self.append_report(end.report)?;
        self.queued.extend(std::mem::take(&mut self.held));
        self.pending_converted = Some(end.chunks);
        self.finishing_source = true;
        Ok(())
    }
    fn finish_client(&mut self) -> Result<(), TransformError> {
        let events: Vec<_> = self.queued.iter().map(|(e, _)| e.clone()).collect();
        for event in events {
            self.observe(&event)?;
            B::ClientEvent::collect(
                self.client
                    .as_mut()
                    .ok_or_else(|| super::invalid("client collector consumed"))?,
                event,
            )?;
        }
        if B::ClientEvent::DONE {
            B::ClientEvent::collect_done(
                self.client
                    .as_mut()
                    .ok_or_else(|| super::invalid("client collector consumed"))?,
            )?;
        }
        let client = B::ClientEvent::collected(
            self.client
                .take()
                .ok_or_else(|| super::invalid("client collector consumed"))?,
        )?;
        self.client_final = Some(client.value.value);
        self.append_report(client.report)?;
        self.eof = true;
        Ok(())
    }
    fn append_report(&mut self, report: Report) -> Result<(), TransformError> {
        for diagnostic in report.diagnostics {
            if !self.report.diagnostics.contains(&diagnostic) {
                if self.report.diagnostics.len() >= self.settings.events.max_events {
                    return Err(super::limit("stream diagnostic limit exceeded"));
                }
                self.report.diagnostics.push(diagnostic);
            }
        }
        Ok(())
    }
}
