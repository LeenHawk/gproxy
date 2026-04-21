use futures_util::TryStreamExt;
use http::Uri;

use crate::response::{
    RetryableUpstreamResponse, UpstreamError, UpstreamResponse, UpstreamStreamingResponse,
};

fn normalize_redirect_path(path: &str) -> &str {
    if path.len() > 1 {
        path.trim_end_matches('/')
    } else {
        path
    }
}

fn safe_provider_redirect_policy(original_uri: Uri) -> wreq::redirect::Policy {
    wreq::redirect::Policy::custom(move |attempt| {
        if attempt.previous.len() > 10 {
            return attempt.error("too many redirects");
        }

        let next = attempt.uri.as_ref();
        let original_scheme = original_uri.scheme_str();
        let next_scheme = next.scheme_str();
        let same_host = original_uri.host() == next.host();
        let scheme_allowed = original_scheme == next_scheme
            || matches!(
                (original_scheme, next_scheme),
                (Some("http"), Some("https"))
            );
        let same_endpoint =
            normalize_redirect_path(original_uri.path()) == normalize_redirect_path(next.path());

        if same_host && scheme_allowed && same_endpoint {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn build_wreq_request(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
    redirect_policy: wreq::redirect::Policy,
) -> Result<wreq::Request, UpstreamError> {
    let (parts, body) = request.into_parts();

    // Provider traffic must not blindly inherit the engine's global
    // redirect policy. On shared reverse-proxy domains, silently
    // following 3xx responses can move a request onto a different
    // provider endpoint while the engine still logs the original URI.
    client
        .request(parts.method, parts.uri)
        .headers(parts.headers)
        .version(parts.version)
        .redirect(redirect_policy)
        .body(body)
        .build()
        .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
}

/// Send an `http::Request<Vec<u8>>` via wreq and return an `UpstreamResponse`.
pub async fn send_request(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<UpstreamResponse, UpstreamError> {
    send_request_with_policy(client, request, wreq::redirect::Policy::limited(10)).await
}

/// Send a provider-bound request via wreq using a conservative redirect
/// policy that only allows same-endpoint canonicalization redirects.
pub async fn send_provider_request(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<UpstreamResponse, UpstreamError> {
    let redirect_policy = safe_provider_redirect_policy(request.uri().clone());
    send_request_with_policy(client, request, redirect_policy).await
}

/// Send an `http::Request<Vec<u8>>` via wreq and keep successful responses as
/// a byte stream. Non-success responses are buffered so retry logic can inspect
/// the body.
pub async fn send_request_stream(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<RetryableUpstreamResponse, UpstreamError> {
    send_request_stream_with_policy(client, request, wreq::redirect::Policy::limited(10)).await
}

/// Streaming counterpart of [`send_provider_request`].
pub async fn send_provider_request_stream(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
) -> Result<RetryableUpstreamResponse, UpstreamError> {
    let redirect_policy = safe_provider_redirect_policy(request.uri().clone());
    send_request_stream_with_policy(client, request, redirect_policy).await
}

async fn send_request_with_policy(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
    redirect_policy: wreq::redirect::Policy,
) -> Result<UpstreamResponse, UpstreamError> {
    let started_at = std::time::Instant::now();
    let wreq_request = build_wreq_request(client, request, redirect_policy)?;

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

async fn send_request_stream_with_policy(
    client: &wreq::Client,
    request: http::Request<Vec<u8>>,
    redirect_policy: wreq::redirect::Policy,
) -> Result<RetryableUpstreamResponse, UpstreamError> {
    let started_at = std::time::Instant::now();
    let wreq_request = build_wreq_request(client, request, redirect_policy)?;

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
