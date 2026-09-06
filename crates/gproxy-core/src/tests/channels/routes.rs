use gproxy_channel_api::Channel;

#[test]
fn copilot_compact_transforms_both_directions_and_settles_chat_usage() {
    use super::super::{block_on, memory::MemoryHost, request, target};
    use crate::control::{FailoverBudget, Plan};
    use crate::{Core, ResponseBody};
    use bytes::Bytes;
    use serde_json::{Value, json};

    let host = MemoryHost::new(false);
    let mut selected = target();
    selected.provider.channel = "copilotcli".into();
    selected.provider.settings = json!({"base_url":"https://upstream.test/copilot-compact"});
    {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "copilotcli".into();
        state.credential.secret =
            json!({"copilot_token":"test-token","copilot_expires_at_ms":i64::MAX});
        state.plan = Some(Plan {
            targets: vec![selected],
            budget: FailoverBudget { max_attempts: 1 },
        });
    }
    let registry =
        gproxy_channel_api::ChannelRegistry::new([
            Box::new(gproxy_channels::CopilotCliChannel) as Box<dyn Channel>
        ])
        .unwrap();
    let core = Core::new(host.clone(), registry).unwrap();
    let mut request = request(false, "copilot-compact");
    request.path = "/v1/responses/compact".into();
    request.body = Bytes::from_static(
        br#"{"model":"alias","input":[{"role":"user","content":"Summarize the history"}]}"#,
    );
    let outcome = block_on(core.execute(&host, request)).unwrap();
    assert!(outcome.status.is_success());
    let ResponseBody::Full(body) = outcome.body else {
        panic!("buffered compact response")
    };
    let response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(response["object"], "response.compaction");
    assert!(!response["output"].as_array().unwrap().is_empty());
    let state = host.state.lock().unwrap();
    assert!(
        state.upstream_requests[0]
            .1
            .ends_with("/copilot-compact/chat/completions")
    );
    let upstream: Value = serde_json::from_slice(&state.upstream_bodies[0]).unwrap();
    assert!(upstream["messages"].is_array());
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(
        (
            state.settlements[0].usage.input_tokens,
            state.settlements[0].usage.output_tokens
        ),
        (10, 5)
    );
}

#[test]
fn every_builtin_default_has_an_executable_target_and_transform() {
    let mut channels = vec![
        &gproxy_channels::OpenAiChannel as &dyn Channel,
        &gproxy_channels::AntigravityChannel,
        &gproxy_channels::ClaudeApiChannel,
        &gproxy_channels::ClaudeCodeChannel,
        &gproxy_channels::GeminiCliChannel,
        &gproxy_channels::ClineChannel,
        &gproxy_channels::CloudflareAiGatewayChannel,
        &gproxy_channels::CodexChannel,
        &gproxy_channels::CopilotCliChannel,
        &gproxy_channels::CustomChannel,
        &gproxy_channels::DashScopeChannel,
        &gproxy_channels::DeepSeekChannel,
        &gproxy_channels::GroqChannel,
        &gproxy_channels::GrokBuildChannel,
        &gproxy_channels::KiroChannel,
        &gproxy_channels::KimiChannel,
        &gproxy_channels::NvidiaChannel,
        &gproxy_channels::OpenCodeChannel,
        &gproxy_channels::OpenRouterChannel,
        &gproxy_channels::AiStudioChannel,
        &gproxy_channels::AzureChannel,
        &gproxy_channels::AwsBedrockChannel,
        &gproxy_channels::VertexChannel,
        &gproxy_channels::VertexExpressChannel,
        &gproxy_channels::WorkBuddyChannel,
        &gproxy_channels::XaiChannel,
        &gproxy_channels::VercelChannel,
    ];
    #[cfg(not(target_arch = "wasm32"))]
    channels.push(&gproxy_channels::ClaudeWebChannel);
    for channel in channels {
        for route in channel.routing_table() {
            if matches!(
                route.action,
                gproxy_channel_api::ChannelRouteAction::Passthrough
                    | gproxy_channel_api::ChannelRouteAction::TransformTo
            ) {
                assert!(
                    crate::attempt::executable(channel, route),
                    "{} has an unexecutable default: {:?}",
                    channel.descriptor().id,
                    route
                );
            }
        }
        for support in channel
            .descriptor()
            .supports
            .iter()
            .chain(channel.routing_table())
        {
            if support.action == gproxy_channel_api::ChannelRouteAction::TransformTo {
                assert!(
                    gproxy_transform::can_transform(support.source, support.target),
                    "{} declares an unwired transform: {:?}",
                    channel.descriptor().id,
                    support
                );
            }
        }
    }
}
