mod channel;
mod memory;
mod services;

use std::future::Future;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{Channel, Disposition};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::json;

use self::memory::MemoryHost;
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{ProviderRef, Target};
use crate::host::CredentialId;
use crate::usage::{Ended, UsageSource};
use crate::{Core, InitError};

#[test]
fn invoke_refreshes_with_version_guard_and_finishes_the_funnel() -> Result<(), InitError> {
    for (conflict, expected_token) in [(false, "Bearer fresh"), (true, "Bearer peer")] {
        let host = MemoryHost::new(conflict);
        let core = core(&host)?;
        let outcome =
            block_on(core.invoke(&host, &target(), request(false, &conflict.to_string())))
                .expect("invoke");
        assert_eq!(outcome.status, StatusCode::OK);
        assert_eq!(outcome.disposition, Disposition::Success);
        assert!(matches!(outcome.body, ResponseBody::Full(_)));

        let state = host.state.lock().expect("state lock");
        assert_eq!(state.lease_calls, 1);
        assert_eq!(state.rotations, [4]);
        assert_eq!(state.credential.version, 5);
        assert_eq!(state.authorizations, [expected_token]);
        assert_eq!(state.settlements.len(), 1);
        let settlement = &state.settlements[0];
        assert_eq!(settlement.usage.input_tokens, 10);
        assert_eq!(settlement.usage.output_tokens, 5);
        assert_eq!(settlement.cost, Decimal::new(2, 5));
        assert_eq!(settlement.source, UsageSource::Upstream);
        assert_eq!(settlement.ended, Ended::Complete);
        assert_eq!(state.captures.len(), 1);
        assert_eq!(state.captures[0].0, Some(StatusCode::OK));
        assert!(state.captures[0].1.is_some());
    }
    Ok(())
}

#[test]
fn streaming_invoke_settles_inline_before_eof_without_a_spawner() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;
    let outcome = block_on(core.invoke(&host, &target(), request(true, "stream"))).expect("invoke");
    let ResponseBody::Stream(mut stream) = outcome.body else {
        panic!("streaming request returned a buffered body");
    };
    block_on(async {
        assert!(stream.next().await.expect("one frame").is_ok());
        assert!(stream.next().await.is_none());
    });

    let state = host.state.lock().expect("state lock");
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].ended, Ended::Complete);
    assert_eq!(state.captures.len(), 1);
    Ok(())
}

fn core(host: &MemoryHost) -> Result<Core<MemoryHost>, InitError> {
    let channels =
        gproxy_channel_api::ChannelRegistry::new([Box::new(host.clone()) as Box<dyn Channel>])
            .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn target() -> Target {
    Target {
        provider: ProviderRef {
            id: 3,
            name: "provider".into(),
            channel: "memory".into(),
            settings: json!({}),
        },
        credential: CredentialId(7),
        upstream_model: "upstream-model".into(),
    }
}

fn request(stream: bool, id: &str) -> RequestCtx {
    RequestCtx {
        request_id: format!("request-{id}"),
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: Bytes::from(format!(r#"{{"model":"alias","stream":{stream}}}"#)),
        mode: RoutingMode::Aggregated,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
