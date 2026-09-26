use super::*;
use crate::{capability::Upstream, transform::TransformErrorKind};

impl<B: FanoutBridge> FanoutStream<B>
where
    B::ClientEvent: FanoutEvent,
{
    /// Starts each unstarted child once and returns incremental client bytes.
    /// Errors retain native receipts in memory; cancellation during a POST
    /// never triggers a second POST.
    pub async fn next<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        if self
            .children
            .iter()
            .any(|child| child.image_resources_required)
        {
            return Err(super::super::missing(
                "fanout URI delivery requires next_with_image_resources",
            ));
        }
        self.next_using(upstream, target, state, &mut super::driver::Plain)
            .await
    }
    pub(super) async fn next_using<
        U: Upstream,
        S: StateStore,
        D: super::driver::ChildNext<B, S>,
    >(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
        driver: &mut D,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        if self.failed {
            return Err(invalid(
                "fanout stream failed; retained children require reconciliation",
            ));
        }
        if self.finished {
            return Ok(None);
        }
        if !self.group_saved {
            let mut record = IdentityStateRecord::new(IdentityRole::Response, state.target.clone());
            record.client_item_id = Some(self.id.clone());
            state
                .save_records(vec![(record, None)], vec![], &mut self.group)
                .await?;
            self.group_saved = true;
        }
        let result = self.next_inner(upstream, target, state, driver).await;
        // StateStore/transport interruption may have an uncertain receipt and is
        // retained for explicit recovery. Proven mapping/limit failure is final.
        if let Err(error) = &result
            && matches!(
                error.kind(),
                TransformErrorKind::InvalidInput
                    | TransformErrorKind::InvalidResult
                    | TransformErrorKind::Limit
                    | TransformErrorKind::Unsupported
            )
        {
            self.fail();
        }
        result
    }
    async fn next_inner<U: Upstream, S: StateStore, D: super::driver::ChildNext<B, S>>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
        driver: &mut D,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        while self.index < self.children.len() {
            if !self.seeded {
                let mut reserved = self.fixed.clone();
                reserved.extend(self.seen.keys().cloned());
                let reserved_budget = self
                    .fixed
                    .len()
                    .checked_add(state.max_records)
                    .ok_or_else(|| limit("fanout reserved identity budget overflow"))?;
                let child = &mut self.children[self.index];
                child
                    .bridge
                    .as_mut()
                    .ok_or_else(|| invalid("child bridge consumed"))?
                    .reserve(IdentityRole::ToolCall, &reserved, reserved_budget)?;
                child.bridge.as_mut().unwrap().reserve(
                    IdentityRole::Response,
                    &self.response_ids,
                    reserved_budget,
                )?;
                child.flow = child.bridge.as_ref().unwrap().identities().clone();
                self.seeded = true;
            }
            if !self.children[self.index].sent
                && let StreamStart::Rejected(_) = self.children[self.index]
                    .start(upstream, target, state)
                    .await?
            {
                return Err(invalid(format!(
                    "fanout child {} rejected request; native receipt retained",
                    self.index
                )));
            }
            if self.children[self.index].finished {
                let child = &self.children[self.index];
                let native = child
                    .native_result()
                    .ok_or_else(|| invalid("finished child lacks native receipt"))?;
                let client = child
                    .client_result()
                    .ok_or_else(|| invalid("finished child lacks acknowledged result"))?;
                let retained = crate::codec::encode_json(&(native, client), self.settings.codec)
                    .map_err(codec_error)?
                    .len();
                self.retained_bytes = self
                    .retained_bytes
                    .checked_add(retained)
                    .ok_or_else(|| limit("fanout retained byte overflow"))?;
                if self.retained_bytes > self.settings.events.max_bytes
                    || self.retained_bytes as u64 > self.settings.codec.max_body_bytes
                {
                    return Err(limit("fanout retained results exceed aggregate budget"));
                }
                for diagnostic in &child.report.diagnostics {
                    if !self.report.diagnostics.contains(diagnostic) {
                        self.report.diagnostics.push(diagnostic.clone());
                    }
                }
                if self.report.diagnostics.len() > self.settings.events.max_events {
                    return Err(limit("fanout diagnostics exceeded"));
                }
                self.response_ids.extend(
                    child
                        .flow
                        .handles()
                        .filter(|handle| handle.role == IdentityRole::Response)
                        .map(|handle| handle.emitted_id),
                );
                self.completed.push(client.clone());
                self.index += 1;
                self.seeded = false;
                continue;
            }
            let chunk = match driver
                .next(&mut self.children[self.index], self.index, state)
                .await
            {
                Ok(chunk) => chunk,
                Err(error) => {
                    // A prior child already exposed this immutable identity.
                    // This remains a proven collision even if child persistence
                    // rejected the alias before its pending event could return.
                    let duplicate = self.children[self.index]
                        .ready
                        .as_ref()
                        .and_then(|ready| ready.chunk.event.as_ref())
                        .is_some_and(|event| {
                            event.tool_declarations(true).iter().any(|(id, _, _)| {
                                self.seen.get(id).is_some_and(|index| *index != self.index)
                            })
                        });
                    if duplicate {
                        return Err(invalid("independent children repeat an immutable tool ID"));
                    }
                    return Err(error);
                }
            };
            let Some(mut event) = chunk.and_then(|v| v.event) else {
                continue;
            };
            for (id, _, _) in event.tool_declarations(true) {
                if self.seen.get(&id).is_some_and(|index| *index != self.index) {
                    return Err(invalid("independent children repeat an immutable tool ID"));
                }
                self.seen.insert(id, self.index);
            }
            if self
                .seen
                .len()
                .checked_add(self.children.len())
                .and_then(|n| n.checked_add(1))
                .is_none_or(|count| count > state.max_records)
            {
                return Err(limit("fanout tool identity budget exceeded"));
            }
            event.project(self.index, &self.id, &mut self.created)?;
            if !event.visible() {
                continue;
            }
            B::ClientEvent::collect(
                self.collector
                    .as_mut()
                    .ok_or_else(|| invalid("aggregate collector consumed"))?,
                event.clone(),
            )?;
            return self.emit(event).map(Some);
        }
        if self.aggregate.is_none() {
            let aggregate = B::ClientEvent::aggregate(
                self.completed.clone(),
                self.id.clone(),
                &mut self.report,
            )?;
            // Collect actual projected chunks plus actual summed usage before
            // accepting the final aggregate. include_usage controls wire only.
            if let Some(tail) = B::ClientEvent::tail(&aggregate, true)? {
                B::ClientEvent::collect(
                    self.collector
                        .as_mut()
                        .ok_or_else(|| invalid("aggregate collector consumed"))?,
                    tail,
                )?;
            }
            if B::ClientEvent::DONE {
                B::ClientEvent::collect_done(self.collector.as_mut().unwrap())?;
            }
            let actual = B::ClientEvent::collected(self.collector.take().unwrap())?;
            if !B::ClientEvent::equivalent(&actual.value.value, &aggregate) {
                return Err(invalid(
                    "fanout actual incremental output differs from completed aggregate",
                ));
            }
            self.tail = B::ClientEvent::tail(&aggregate, self.emit_usage)?;
            self.aggregate = Some(aggregate);
        }
        if !self.tail_emitted {
            self.tail_emitted = true;
            if let Some(tail) = self.tail.take() {
                return self.emit(tail).map(Some);
            }
        }
        let bytes = self.encoder.finish::<B::ClientEvent>()?;
        self.finished = true;
        Ok(Some(StreamChunk {
            event: None,
            bytes,
            finished: true,
        }))
    }
    pub async fn collect<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<crate::transform::Converted<ClientFull<B>>, TransformError> {
        while self.next(upstream, target, state).await?.is_some() {}
        Ok(crate::transform::Converted {
            value: self
                .client_result()
                .ok_or_else(|| invalid("aggregate not acknowledged"))?
                .clone(),
            report: self.report.clone(),
        })
    }
}
