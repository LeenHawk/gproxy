use std::{
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream, UpstreamConnection,
    },
    connection::{HeaderMap, Method, StatusCode},
};

fn limits() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: Duration::from_secs(5),
        stream_idle: Duration::from_secs(1),
        read_bytes: 1024,
        write_bytes: 1024,
        ws_frame_bytes: 1024,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(std::task::Waker::noop());
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous contract fake unexpectedly pending"),
    }
}

fn request(path: &str, body: HttpBody) -> WireRequest<HttpBody> {
    let (path, query) = path
        .split_once('?')
        .map_or((path, None), |(p, q)| (p, Some(q.to_owned())));
    WireRequest {
        method: Method::GET,
        path: path.to_owned(),
        query,
        headers: HeaderMap::new(),
        body,
    }
}

type RequestLocation = (String, Option<String>);

#[derive(Clone)]
struct PagingUpstream {
    paths: Arc<Mutex<Vec<RequestLocation>>>,
}

impl Upstream for PagingUpstream {
    type Target = String;

    fn send<'a>(
        &'a self,
        _: &'a Self::Target,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        self.paths
            .lock()
            .unwrap()
            .push((request.path.clone(), request.query.clone()));
        let body = if let Some(item) = request.path.strip_prefix("/batch/") {
            Bytes::from(format!("item={item}"))
        } else if request.path == "/items" && request.query.as_deref() == Some("page=1") {
            Bytes::from_static(b"items=one,next=/items?page=2")
        } else {
            Bytes::from_static(b"items=two,next=")
        };
        Box::pin(async move {
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(body),
            })
        })
    }

    fn connect<'a>(
        &'a self,
        _: &'a Self::Target,
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "connect not used by this fake",
            ))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        limits()
    }
}

#[test]
fn pagination_uses_next_request_and_preserves_order() {
    let upstream = PagingUpstream {
        paths: Arc::new(Mutex::new(Vec::new())),
    };
    let target = "origin-a".to_owned();
    let mut path = "/items?page=1".to_owned();
    let mut items = Vec::new();
    loop {
        let response =
            block_on(upstream.send(&target, request(&path, HttpBody::Bytes(Bytes::new()))))
                .unwrap();
        let HttpBody::Bytes(bytes) = response.body else {
            panic!("fake is buffered")
        };
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let (item, next) = text.split_once(",next=").unwrap();
        items.push(item.strip_prefix("items=").unwrap().to_owned());
        if next.is_empty() {
            break;
        }
        path = next.to_owned();
    }
    assert_eq!(items, ["one", "two"]);
    assert_eq!(
        &*upstream.paths.lock().unwrap(),
        &[
            ("/items".to_owned(), Some("page=1".to_owned())),
            ("/items".to_owned(), Some("page=2".to_owned()))
        ]
    );
}

#[test]
fn batch_splitting_gathers_results_in_input_order() {
    let upstream = PagingUpstream {
        paths: Arc::new(Mutex::new(Vec::new())),
    };
    let target = "origin-a".to_owned();
    let mut gathered = Vec::new();
    for batch in [[0, 1], [2, 3]] {
        for item in batch {
            let response = block_on(upstream.send(
                &target,
                request(
                    &format!("/batch/{item}"),
                    HttpBody::Bytes(Bytes::from(format!("item={item}"))),
                ),
            ))
            .unwrap();
            let HttpBody::Bytes(body) = response.body else {
                panic!("fake is buffered")
            };
            gathered.push(String::from_utf8(body.to_vec()).unwrap());
        }
    }
    assert_eq!(gathered, ["item=0", "item=1", "item=2", "item=3"]);
}
