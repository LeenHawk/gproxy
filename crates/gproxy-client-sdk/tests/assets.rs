#![cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]

mod support;

use futures_util::StreamExt;

use gproxy_client_sdk::*;

use serde_json::Value;

use std::{cell::Cell, rc::Rc, sync::Mutex};

fn reference() -> Asset {
    Asset::Reference {
        reference: "blob:sha256".into(),
    }
}

fn image(source: Asset, mime: Option<&str>) -> Content {
    Content::Image(Image {
        source,
        media_type: mime.map(str::to_owned),
        detail: Detail::Auto,
    })
}

fn request(dialect: Dialect, content: Content) -> GenerateRequest {
    support::request(
        dialect,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(content)],
        }],
    )
}

fn url(url: &str, mime: Option<&str>, size: Option<u64>) -> ResolvedAsset {
    ResolvedAsset::Url {
        url: url.into(),
        media_type: mime.map(str::to_owned),
        byte_len: size,
    }
}

fn bytes(mime: &str) -> ResolvedAsset {
    ResolvedAsset::Bytes {
        bytes: vec![1, 2, 3],
        media_type: mime.into(),
    }
}

fn assert_invalid<T>(result: Result<T>) {
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::InvalidData,
            ..
        })
    ));
}

struct Script {
    sent: Mutex<Vec<Value>>,
    stream: bool,
}

impl Backend for Script {
    fn send(
        &self,
        _: protocol::OperationKey,
        _: Option<&str>,
        request: protocol::WireRequest<protocol::HttpBody>,
    ) -> protocol::capability::CapabilityFuture<
        '_,
        Result<protocol::WireResponse<protocol::HttpBody>>,
    > {
        Box::pin(async move {
            let raw = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.sent
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&raw).unwrap());
            let native = support::openai();
            let body = if self.stream {
                support::openai_events(&native)
                    .into_iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>()
            } else {
                native.to_string()
            };
            Ok(protocol::WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: protocol::HttpBody::Bytes(body.into_bytes().into()),
            })
        })
    }
}

#[path = "assets/capabilities.rs"]
mod capabilities;

#[path = "assets/client.rs"]
mod client;

#[path = "assets/errors.rs"]
mod errors;

#[path = "assets/policy.rs"]
mod policy;

#[path = "assets/preparation.rs"]
mod preparation;
