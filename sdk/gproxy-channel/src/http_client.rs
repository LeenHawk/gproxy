use futures_util::TryStreamExt;

use crate::response::{
    RetryableUpstreamResponse, UpstreamError, UpstreamResponse, UpstreamStreamingResponse,
};

/// Send an `http::Request<Vec<u8>>` via wreq and return an `UpstreamResponse`.
pub async fn send_request(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<UpstreamResponse, UpstreamError> {
    let started_at = std::time::Instant::now();
    let wreq_request = wreq::Request::from(request);

    let response = client
        .execute(wreq_request)
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    let initial_latency_ms = started_at.elapsed().as_millis() as u64;

    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?
        .to_vec();
    let total_latency_ms = started_at.elapsed().as_millis() as u64;

    Ok(UpstreamResponse {
        status,
        headers,
        body,
        initial_latency_ms,
        total_latency_ms,
    })
}

/// Send an `http::Request<Vec<u8>>` via wreq and keep successful responses as
/// a byte stream. Non-success responses are buffered so retry logic can inspect
/// the body.
pub async fn send_request_stream(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<RetryableUpstreamResponse, UpstreamError> {
    let started_at = std::time::Instant::now();
    let wreq_request = wreq::Request::from(request);

    let response = client
        .execute(wreq_request)
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    let initial_latency_ms = started_at.elapsed().as_millis() as u64;

    let status = response.status().as_u16();
    let headers = response.headers().clone();

    if (200..=299).contains(&status) {
        let body = response
            .bytes_stream()
            .map_err(|e| UpstreamError::Http(e.to_string()));
        return Ok(RetryableUpstreamResponse::Streaming(
            UpstreamStreamingResponse {
                status,
                headers,
                body: Box::pin(body),
                initial_latency_ms,
                stream_start: started_at,
            },
        ));
    }

    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?
        .to_vec();
    let total_latency_ms = started_at.elapsed().as_millis() as u64;

    Ok(RetryableUpstreamResponse::Buffered(UpstreamResponse {
        status,
        headers,
        body,
        initial_latency_ms,
        total_latency_ms,
    }))
}
