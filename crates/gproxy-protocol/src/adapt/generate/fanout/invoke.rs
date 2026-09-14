use super::super::{GenerationStateAccess, transport};
use super::*;
use super::{
    edges::Edge,
    journal::{Binding, ChildBinding, Journal, SavedChild},
    output::{Client, ToolsOnly},
};
use crate::{
    capability::{StateStore, Upstream},
    transform::{Converted, Report, identity::IdentityStateRecord},
};
use std::collections::BTreeSet;
impl<A: Edge> Fanout<A> {
    fn journal<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
        limits: CodecLimits,
    ) -> Result<Journal, TransformError> {
        self.endpoint.validate()?;
        if self.children.len() > self.options.max_children || self.children.len() < 2 {
            return Err(limit());
        }
        let mut children = Vec::new();
        for child in &self.children {
            let ids = child.identities();
            state.validate_target(ids.request_policy.dialect, child.model())?;
            children.push(ChildBinding {
                request: encode(child.request(), limits)?,
                namespaces: (ids.request.namespace(), ids.response.namespace()),
                policies: (ids.request_policy.clone(), ids.response_policy.clone()),
            });
        }
        Ok(Journal {
            schema: 1,
            binding: Binding {
                id: self.group_id.clone(),
                original: self.original.clone(),
                target: state.target.clone(),
                conversation: state.conversation_key.clone(),
                expires_at: state.expires_at,
                endpoint: (
                    self.endpoint.path.clone(),
                    self.endpoint.query.clone(),
                    self.endpoint
                        .headers
                        .iter()
                        .map(|(k, v)| (k.as_str().into(), v.as_bytes().to_vec()))
                        .collect(),
                ),
                children,
            },
            children: vec![SavedChild::default(); self.children.len()],
            group_identities: Default::default(),
            exposed: None,
        })
    }
    pub(super) async fn run<U: Upstream, S: StateStore>(
        &mut self,
        upstream: (&U, &U::Target),
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        progress: &mut FanoutProgress<A::Native>,
        mut facts: impl FnMut(usize, &A::Native) -> Result<A::Facts, TransformError>,
        resume: bool,
    ) -> Result<Converted<A::Client>, TransformError> {
        let (upstream, target) = upstream;
        let expected = self.journal(state, limits)?;
        if expected
            .binding
            .children
            .iter()
            .any(|child| child.request.len() as u64 > upstream.limits().write_bytes)
        {
            return Err(limit());
        }
        if resume {
            journal::load(expected, state, limits, progress).await?;
        } else {
            if progress.journal.is_some()
                || !progress.children.is_empty()
                || progress.version.is_some()
            {
                return Err(conflict(
                    "invoke requires fresh progress; resume the existing journal",
                ));
            }
            progress.children = (0..self.children.len())
                .map(|_| GenerationProgress::default())
                .collect();
            progress.journal = Some(expected);
            journal::save(state, limits, progress).await?;
        }
        // Persist any retained native result before considering another child.
        for index in 0..self.children.len() {
            if let Some(raw) = &progress.children[index].raw_response {
                progress.journal.as_mut().expect("initialized").children[index].raw =
                    Some(journal::Raw::from_wire(raw));
            }
        }
        journal::save(state, limits, progress).await?;
        for index in 0..self.children.len() {
            if progress.children[index].raw_response.is_some() {
                transport::recover_native(&mut progress.children[index], limits)?;
                continue;
            }
            if progress.children[index].send_started {
                return Err(conflict(format!(
                    "child {index} was started without a retained successful result; never repeat its POST"
                )));
            }
            progress.journal.as_mut().expect("initialized").children[index].started = true;
            journal::save(state, limits, progress).await?;
            let sent = transport::send(
                upstream,
                target,
                self.endpoint.clone(),
                self.children[index].request().clone(),
                limits,
                &mut progress.children[index],
            )
            .await;
            if let Some(raw) = &progress.children[index].raw_response {
                progress.journal.as_mut().expect("initialized").children[index].raw =
                    Some(journal::Raw::from_wire(raw));
                journal::save(state, limits, progress).await?;
            }
            if !sent? {
                return Err(TransformError::invalid_result(
                    "fanout.child",
                    format!(
                        "child {index} rejected the request; native HTTP result retained, group is incomplete"
                    ),
                ));
            }
        }
        let mut report = Report::default();
        let mut mapped = Vec::new();
        let mut flows = Vec::new();
        let mut natives = Vec::new();
        let mut seen = BTreeSet::new();
        for (index, child) in self.children.iter_mut().enumerate() {
            let native = transport::recover_native(&mut progress.children[index], limits)?;
            let supplied = facts(index, &native)?;
            let converted = child.convert(native.clone(), supplied)?;
            report
                .diagnostics
                .extend(child.report().diagnostics.clone());
            report.diagnostics.extend(converted.report.diagnostics);
            mapped.push(converted.value);
            natives.push(native);
        }
        let reserved = output::reserved(&mapped)?;
        for (index, (native, client)) in natives.iter().zip(&mut mapped).enumerate() {
            flows.push(output::normalize(
                native,
                client,
                self.children[index].identities(),
                &mut seen,
                &reserved,
                state.max_records,
                self.children[index].signed_bindings(),
            )?);
        }
        if seen
            .len()
            .checked_add(1)
            .is_none_or(|count| count > state.max_records)
        {
            return Err(limit());
        }
        // Aggregate validation and body limits run before client identity writes.
        let value = A::Client::aggregate(mapped.clone(), self.group_id.clone(), &mut report)?;
        let encoded = encode(&value, limits)?;
        if progress
            .journal
            .as_ref()
            .expect("initialized")
            .exposed
            .as_ref()
            .is_some_and(|v| v != &encoded)
        {
            return Err(conflict("aggregate changed after first client exposure"));
        }
        for (index, ((native, client), flow)) in natives.iter().zip(&mapped).zip(&flows).enumerate()
        {
            let result = if let Some(bindings) = self.children[index].signed_bindings() {
                state
                    .save_pair_with_bound_ids(
                        native,
                        &ToolsOnly(client),
                        flow,
                        bindings,
                        &mut progress.children[index],
                    )
                    .await
            } else {
                state
                    .save_pair(
                        native,
                        &ToolsOnly(client),
                        flow,
                        &mut progress.children[index],
                    )
                    .await
            };
            progress.journal.as_mut().expect("initialized").children[index].identities =
                journal::pack(&progress.children[index].saved_identities);
            journal::save(state, limits, progress).await?;
            result?;
        }
        let mut record = IdentityStateRecord::new(IdentityRole::Response, state.target.clone());
        record.client_item_id = Some(self.group_id.clone());
        // No single native response owns an aggregate ID. Its durable journal
        // contains every original response and ordered request association.
        let result = state
            .save_records(vec![(record, None)], vec![], &mut progress.group)
            .await;
        progress
            .journal
            .as_mut()
            .expect("initialized")
            .group_identities = journal::pack(&progress.group.saved_identities);
        journal::save(state, limits, progress).await?;
        result?;
        progress.journal.as_mut().expect("initialized").exposed = Some(encoded);
        journal::save(state, limits, progress).await?;
        Ok(Converted { value, report })
    }
}
