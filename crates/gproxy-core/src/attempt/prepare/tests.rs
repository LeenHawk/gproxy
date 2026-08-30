use std::sync::Arc;

use gproxy_channel_api::ChannelSupport;
use gproxy_protocol::{Operation, OperationKey, WireFamily};

use super::route_support;
use crate::control::{ProviderRef, Target, TargetRules};
use crate::host::CredentialId;
use crate::routing::{CompiledRoutingRule, RoutingImplementation};

#[test]
fn declared_unsupported_route_cannot_be_enabled() {
    let key = OperationKey::family(Operation::CreateEmbedding, WireFamily::OpenAi);
    let target = Target {
        provider: ProviderRef {
            id: 1,
            name: "test".into(),
            channel: "test".into(),
            settings: serde_json::Value::Null,
            fingerprint: None,
            proxy_url: None,
        },
        credential: CredentialId(1),
        upstream_model: "test".into(),
        tier: 0,
        rules: TargetRules {
            routing: Arc::from([CompiledRoutingRule {
                operation: key.operation,
                kind: key.kind,
                implementation: RoutingImplementation::Passthrough,
                destination: None,
                sort_order: 0,
            }]),
            process: Arc::from([]),
        },
    };

    assert!(route_support(&target, ChannelSupport::unsupported(key)).is_none());
}
