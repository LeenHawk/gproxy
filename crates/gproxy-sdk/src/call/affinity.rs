//! Route affinity binds only the provider and upstream model. Core owns credentials.
use crate::{Gproxy, resolve::Plan};
use gproxy_core::{ExecutionTarget, SessionIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(super) struct RouteAffinity {
    key: String,
}

#[derive(Serialize, Deserialize)]
struct Pin {
    provider_id: String,
    upstream_model: Option<String>,
}

impl RouteAffinity {
    pub(super) async fn apply<C>(
        gproxy: &Gproxy<C>,
        plan: &mut Plan,
        scope: &str,
        session: &SessionIdentity,
    ) -> Option<Self> {
        if !session.is_stable() {
            return None;
        }
        let config = plan.affinity.as_ref()?;
        let mut digest = Sha256::new();
        // Length prefixes keep caller-controlled session/scope strings unambiguous.
        for part in [
            config.route_id.as_bytes(),
            scope.as_bytes(),
            &[session.source as u8],
            session.id.as_bytes(),
        ] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part);
        }
        let suffix: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let affinity = Self {
            key: format!("gproxy-sdk:route-affinity:v1:{suffix}"),
        };
        let pin = match gproxy.cache().get(&affinity.key).await {
            Ok(entry) => entry.and_then(|e| serde_json::from_slice::<Pin>(&e.value).ok()),
            Err(error) => {
                tracing::warn!(%error, "route affinity read failed");
                None
            }
        };
        if let Some(pin) = pin
            && let Some(index) = plan.targets.iter().position(|target| {
                target
                    .member_id
                    .as_ref()
                    .is_some_and(|id| config.preferred_members.contains(id))
                    && target.provider.entity.id == pin.provider_id
                    && target.upstream_model == pin.upstream_model
            })
            && index > 0
        {
            let target = plan.targets.remove(index);
            plan.targets.insert(0, target);
        }
        Some(affinity)
    }

    pub(super) async fn commit<C>(&self, gproxy: &Gproxy<C>, target: &ExecutionTarget) {
        let value = serde_json::to_vec(&Pin {
            provider_id: target.provider.entity.id.clone(),
            upstream_model: target.upstream_model.clone(),
        })
        .expect("route pin contains only strings");
        if let Err(error) = gproxy
            .cache()
            .put(&self.key, value, Duration::from_secs(24 * 60 * 60))
            .await
        {
            // An affinity cache error must not discard a successful upstream answer.
            tracing::warn!(%error, "route affinity write failed");
        }
    }
}
