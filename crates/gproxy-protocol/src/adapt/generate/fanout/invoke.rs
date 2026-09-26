use super::super::{GenerationStateAccess, transport};
use super::*;
use super::{
    edges::Edge,
    output::{Client, ToolsOnly},
};
use crate::{
    capability::{StateStore, Upstream},
    transform::{Converted, Report, identity::IdentityStateRecord},
};
use std::collections::BTreeSet;

impl<A: Edge> Fanout<A> {
    pub(super) async fn run<U: Upstream, S: StateStore>(
        &mut self,
        upstream: (&U, &U::Target),
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        progress: &mut FanoutProgress<A::Native>,
        mut facts: impl FnMut(usize, &A::Native) -> Result<A::Facts, TransformError>,
    ) -> Result<Converted<A::Client>, TransformError> {
        let (upstream, target) = upstream;
        if self.children.len() > self.options.max_children || self.children.len() < 2 {
            return Err(limit());
        }
        // Every child must fit the upstream before the first one is sent, so an
        // oversized candidate cannot leave the group half-posted.
        for child in &self.children {
            if encode(child.request(), limits)?.len() as u64 > upstream.limits().write_bytes {
                return Err(limit());
            }
        }
        // A group runs once. Retained progress may hold a started child whose
        // result never came back, and nothing can tell whether its POST landed,
        // so neither a second run nor reused progress may send again.
        if self.started || !progress.children.is_empty() {
            return Err(conflict(
                "fanout already started; a started POST is never repeated",
            ));
        }
        self.started = true;
        progress.children = (0..self.children.len())
            .map(|_| GenerationProgress::default())
            .collect();
        for index in 0..self.children.len() {
            let sent = transport::send(
                upstream,
                target,
                self.endpoint.clone(),
                self.children[index].request().clone(),
                limits,
                &mut progress.children[index],
            )
            .await?;
            if !sent {
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
        encode(&value, limits)?;
        for (index, ((native, client), flow)) in natives.iter().zip(&mapped).zip(&flows).enumerate()
        {
            if let Some(bindings) = self.children[index].signed_bindings() {
                state
                    .save_pair_with_bound_ids(
                        native,
                        &ToolsOnly(client),
                        flow,
                        bindings,
                        &mut progress.children[index],
                    )
                    .await?;
            } else {
                state
                    .save_pair(
                        native,
                        &ToolsOnly(client),
                        flow,
                        &mut progress.children[index],
                    )
                    .await?;
            }
        }
        let mut record = IdentityStateRecord::new(IdentityRole::Response, state.target.clone());
        record.client_item_id = Some(self.group_id.clone());
        // No single native response owns an aggregate ID, so its record links
        // none; the child records above carry each native response.
        state
            .save_records(vec![(record, None)], vec![], &mut progress.group)
            .await?;
        Ok(Converted { value, report })
    }
}
