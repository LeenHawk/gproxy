use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{Frame, NormalizedUsage, StreamDecoder, StreamEnd, TransportError};
use gproxy_protocol::ContentGenerationKind;

use crate::boundary::ByteStream;

pub(crate) struct BodyFailure {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
    pub error: TransportError,
}

pub(crate) struct CollectedStream {
    pub response: http::Response<Bytes>,
    pub usage: Option<NormalizedUsage>,
    pub actual_service_tier: Option<String>,
    pub capture_body: Bytes,
}

pub(crate) async fn collect(
    response: http::Response<ByteStream>,
) -> Result<http::Response<Bytes>, BodyFailure> {
    let (parts, mut stream) = response.into_parts();
    let mut body = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => body.extend_from_slice(&chunk),
            Err(error) => {
                return Err(BodyFailure {
                    status: parts.status,
                    headers: parts.headers,
                    body: body.freeze(),
                    error,
                });
            }
        }
    }
    Ok(http::Response::from_parts(parts, body.freeze()))
}

pub(crate) async fn collect_stream(
    response: http::Response<ByteStream>,
    mut decoder: Option<Box<dyn StreamDecoder>>,
    kind: ContentGenerationKind,
) -> Result<CollectedStream, BodyFailure> {
    let (mut parts, mut stream) = response.into_parts();
    let mut capture = BytesMut::new();
    let mut collector = match gproxy_transform::ResponseCollector::new(kind) {
        Ok(collector) => collector,
        Err(error) => {
            return Err(BodyFailure {
                status: parts.status,
                headers: parts.headers,
                body: Bytes::new(),
                error: TransportError::Interrupted(error.to_string()),
            });
        }
    };
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                return Err(BodyFailure {
                    status: parts.status,
                    headers: parts.headers,
                    body: capture.freeze(),
                    error,
                });
            }
        };
        capture.extend_from_slice(&chunk);
        let frames = match decoder.as_mut() {
            Some(decoder) => decoder.push(chunk),
            None => Ok(vec![Frame(chunk)]),
        };
        let frames = match frames {
            Ok(frames) => frames,
            Err(error) => {
                return Err(BodyFailure {
                    status: parts.status,
                    headers: parts.headers,
                    body: capture.freeze(),
                    error: TransportError::Interrupted(error.to_string()),
                });
            }
        };
        for frame in frames {
            if let Err(error) = collector.push(frame.0) {
                return Err(BodyFailure {
                    status: parts.status,
                    headers: parts.headers,
                    body: capture.freeze(),
                    error: TransportError::Interrupted(error.to_string()),
                });
            }
        }
    }
    let tail = match decoder.as_mut() {
        Some(decoder) => decoder
            .finish(StreamEnd::Complete)
            .map_err(|error| error.to_string()),
        None => Ok(Default::default()),
    };
    let tail = match tail {
        Ok(tail) => tail,
        Err(error) => {
            return Err(BodyFailure {
                status: parts.status,
                headers: parts.headers,
                body: capture.freeze(),
                error: TransportError::Interrupted(error),
            });
        }
    };
    for frame in tail.frames {
        if let Err(error) = collector.push(frame.0) {
            return Err(BodyFailure {
                status: parts.status,
                headers: parts.headers,
                body: capture.freeze(),
                error: TransportError::Interrupted(error.to_string()),
            });
        }
    }
    let body = match collector
        .finish()
        .and_then(|response| response.into_bytes())
    {
        Ok(body) => body,
        Err(error) => {
            return Err(BodyFailure {
                status: parts.status,
                headers: parts.headers,
                body: capture.freeze(),
                error: TransportError::Interrupted(error.to_string()),
            });
        }
    };
    parts.headers.remove(http::header::CONTENT_LENGTH);
    parts.headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    Ok(CollectedStream {
        response: http::Response::from_parts(parts, body),
        usage: tail.usage,
        actual_service_tier: tail.actual_service_tier,
        capture_body: capture.freeze(),
    })
}
