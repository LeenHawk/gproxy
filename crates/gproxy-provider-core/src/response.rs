use std::io;
use std::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, StatusCode};

#[derive(Debug)]
pub enum ProxyResponse {
    Json {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        body: StreamBody,
    },
}

pub struct StreamBody {
    pub content_type: &'static str,
    pub stream: Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send>>,
}

impl std::fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamBody")
            .field("content_type", &self.content_type)
            .field("stream", &"<opaque>")
            .finish()
    }
}

impl StreamBody {
    pub fn new<S>(content_type: &'static str, stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, io::Error>> + Send + 'static,
    {
        Self {
            content_type,
            stream: Box::pin(stream),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamPassthroughError {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl UpstreamPassthroughError {
    pub fn new(status: StatusCode, headers: HeaderMap, body: Bytes) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn from_status(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<Bytes>) -> Self {
        Self::from_status(StatusCode::SERVICE_UNAVAILABLE, message)
    }
}
