use std::sync::Arc;

use gproxy_core::{ControlPlane, CoreError, Plan, RoutingMode};

use super::SnapshotControl;

impl SnapshotControl {
    pub(crate) fn authorization_model(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
    ) -> Option<String> {
        let model = model?;
        let snapshot = self.snapshot.load();
        Some(match mode {
            RoutingMode::Aggregated => model.to_owned(),
            RoutingMode::Scoped { provider } => format!("{provider}/{model}"),
            RoutingMode::Namespace { namespace } => format!("{namespace}/{model}"),
            RoutingMode::Named { name } => {
                if snapshot.namespaces.contains_key(&name.to_ascii_lowercase()) {
                    format!("{name}/{model}")
                } else if snapshot.route_names.contains_key(name) {
                    // A named route ignores the body model when selecting its members.
                    name.clone()
                } else {
                    format!("{name}/{model}")
                }
            }
        })
    }

    pub(crate) fn catalogue_plan(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError> {
        let mut preview = self.clone();
        preview.rotation = Arc::default();
        preview.resolve(model, mode, affinity)
    }
}
