use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::Stream;
use gproxy_channel_api::{BoxFuture, TransportError};

use super::DownstreamCapture;
use crate::host::AppHost;

pub(super) fn wrap(
    host: AppHost,
    capture: DownstreamCapture,
    body: &mut gproxy_core::ResponseBody,
) {
    let gproxy_core::ResponseBody::Stream(stream) = body else {
        return;
    };
    let upstream = std::mem::replace(stream, Box::pin(futures_util::stream::empty()));
    *stream = Box::pin(CapturingStream {
        upstream,
        host: Some(host),
        capture: Some(capture),
        body: Vec::new(),
        write: None,
    });
}

struct CapturingStream {
    upstream: gproxy_core::ByteStream,
    host: Option<AppHost>,
    capture: Option<DownstreamCapture>,
    body: Vec<u8>,
    write: Option<BoxFuture<'static, ()>>,
}

impl Stream for CapturingStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(write) = &mut self.write {
            return match write.as_mut().poll(cx) {
                Poll::Ready(()) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            };
        }
        match self.upstream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                self.body.extend_from_slice(&bytes);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error))),
            Poll::Ready(None) => {
                let host = self.host.take().expect("capture host exists");
                let capture = self.capture.take().expect("capture state exists");
                let body = std::mem::take(&mut self.body);
                self.write = Some(Box::pin(async move {
                    super::backfill(&host, capture, &body).await;
                }));
                self.poll_next(cx)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
