use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::affinity::{CacheAffinityHint, CacheAffinityPool};
use crate::channel::Channel;
use crate::health::CredentialHealth;
use crate::request::PreparedRequest;
use crate::response::{
    ResponseClassification, RetryableUpstreamResponse, UpstreamError, UpstreamResponse,
    UpstreamStreamingResponse,
};
use tracing::Instrument;

// ---------------------------------------------------------------------------
// RetryableResult trait — abstracts buffered vs streaming response handling
// ---------------------------------------------------------------------------

/// Action determined after inspecting a raw upstream response.
enum RetryAction<T> {
    /// 2xx streaming — return immediately, body cannot be inspected.
    ImmediateSuccess { status: u16, output: T },
    /// Body is buffered and can be classified for retry decisions.
    Classifiable(UpstreamResponse),
}

/// Abstracts over buffered (`UpstreamResponse`) and streaming
/// (`RetryableUpstreamResponse`) so the retry loop can be written once.
trait RetryableResult: Sized {
    /// The caller's final success type.
    type Output;

    /// Inspect the raw response and decide whether it's an immediate success
    /// (streaming 2xx) or needs classification.
    fn into_retry_action(self) -> RetryAction<Self::Output>;

    /// Wrap a fully-buffered response into the caller's output type.
    /// Used for Success and PermanentError paths after classification.
    fn wrap_buffered(response: UpstreamResponse) -> Self::Output;
}

impl RetryableResult for UpstreamResponse {
    type Output = UpstreamResponse;

    fn into_retry_action(self) -> RetryAction<Self::Output> {
        RetryAction::Classifiable(self)
    }

    fn wrap_buffered(response: UpstreamResponse) -> Self::Output {
        response
    }
}

impl RetryableResult for RetryableUpstreamResponse {
    type Output = UpstreamStreamingResponse;

    fn into_retry_action(self) -> RetryAction<Self::Output> {
        match self {
            RetryableUpstreamResponse::Streaming(s) => RetryAction::ImmediateSuccess {
                status: s.status,
                output: s,
            },
            RetryableUpstreamResponse::Buffered(b) => RetryAction::Classifiable(b),
        }
    }

    fn wrap_buffered(response: UpstreamResponse) -> Self::Output {
        UpstreamStreamingResponse {
            status: response.status,
            headers: response.headers,
            body: Box::pin(futures_util::stream::once(async move {
                Ok(bytes::Bytes::from(response.body))
            })),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Captured wire-level metadata about the upstream request that actually
/// went out for the credential that succeeded (or returned a permanent
/// error). Used by the engine to populate `UpstreamRequestMeta` for the
/// upstream-request log table without re-running `prepare_request`.
#[derive(Debug, Clone, Default)]
pub struct UpstreamAttemptMeta {
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<Vec<u8>>,
}

/// Result from credential-rotating retry, including which credential succeeded.
pub struct RetryResult<T> {
    pub output: T,
    pub credential_index: usize,
    pub attempt_meta: UpstreamAttemptMeta,
}

/// Failure from credential-rotating retry, carrying diagnostics for the
/// last attempt so the caller can persist a useful upstream-request log
/// row even when no credential succeeded.
///
/// Produced by the retry loop on `AuthDead`, exhausted `RateLimited`,
/// `TransientError`, and `AllCredentialsExhausted` paths. Retains the
/// real upstream URL, forwarded request headers/body, the upstream
/// response status (typically the auth failure code), response headers,
/// and response body from the final attempt.
pub struct RetryFailure {
    pub error: UpstreamError,
    pub last_attempt: Option<crate::response::FailedUpstreamAttempt>,
}

impl RetryFailure {
    pub fn bare(error: UpstreamError) -> Self {
        Self {
            error,
            last_attempt: None,
        }
    }
}

/// Parameters for credential-rotating retry.
pub struct RetryContext<'a, C: Channel> {
    pub channel: &'a C,
    pub credentials: &'a mut [(C::Credential, C::Health)],
    pub settings: &'a C::Settings,
    pub request: &'a PreparedRequest,
    pub affinity_hint: Option<&'a CacheAffinityHint>,
    pub affinity_pool: &'a CacheAffinityPool,
    pub round_robin_cursor: &'a AtomicUsize,
    pub max_retries: u32,
    pub http_client: &'a wreq::Client,
    /// Browser-impersonating client for credentials that need cookie auth.
    /// Falls back to `http_client` when `None`.
    pub spoof_client: Option<&'a wreq::Client>,
    /// Force a specific credential (e.g. for file operations bound to a credential).
    pub forced_credential: Option<usize>,
}

/// Retry a request across multiple credentials.
///
/// For each eligible credential, tries up to `max_retries` times on 429
/// without `retry-after`. If 429 includes `retry-after`, the credential
/// is marked with a cooldown and skipped immediately.
///
/// On 401/403 (AuthDead), calls `channel.refresh_credential` to attempt
/// a token refresh. If refresh succeeds, retries once. If the retry also
/// fails with AuthDead, the credential is marked dead.
///
/// The caller provides a `send` closure that performs the actual HTTP request.
pub async fn retry_with_credentials<C, F, Fut>(
    ctx: RetryContext<'_, C>,
    send: F,
) -> Result<RetryResult<UpstreamResponse>, RetryFailure>
where
    C: Channel,
    F: Fn(&wreq::Client, http::Request<Vec<u8>>) -> Fut,
    Fut: std::future::Future<Output = Result<UpstreamResponse, UpstreamError>>,
{
    let span = tracing::info_span!(
        "retry_with_credentials",
        model = ctx.request.model.as_deref().unwrap_or(""),
        credentials = ctx.credentials.len(),
        max_retries = ctx.max_retries,
    );
    retry_common_inner(ctx, send).instrument(span).await
}

/// Retry a request across multiple credentials while preserving successful
/// upstream bodies as a stream.
pub async fn retry_with_credentials_stream<C, F, Fut>(
    ctx: RetryContext<'_, C>,
    send: F,
) -> Result<RetryResult<UpstreamStreamingResponse>, RetryFailure>
where
    C: Channel,
    F: Fn(&wreq::Client, http::Request<Vec<u8>>) -> Fut,
    Fut: std::future::Future<Output = Result<RetryableUpstreamResponse, UpstreamError>>,
{
    let span = tracing::info_span!(
        "retry_with_credentials_stream",
        model = ctx.request.model.as_deref().unwrap_or(""),
        credentials = ctx.credentials.len(),
        max_retries = ctx.max_retries,
    );
    retry_common_inner(ctx, send).instrument(span).await
}

// ---------------------------------------------------------------------------
// Unified retry loop
// ---------------------------------------------------------------------------

async fn retry_common_inner<C, F, Fut, R>(
    ctx: RetryContext<'_, C>,
    send: F,
) -> Result<RetryResult<R::Output>, RetryFailure>
where
    C: Channel,
    F: Fn(&wreq::Client, http::Request<Vec<u8>>) -> Fut,
    Fut: std::future::Future<Output = Result<R, UpstreamError>>,
    R: RetryableResult,
{
    let RetryContext {
        channel,
        credentials,
        settings,
        request,
        affinity_hint,
        affinity_pool,
        round_robin_cursor,
        max_retries,
        http_client,
        spoof_client,
        forced_credential,
    } = ctx;

    let model = request.model.as_deref();

    // Filter to eligible credentials
    let eligible: Vec<usize> = credentials
        .iter()
        .enumerate()
        .filter(|(_, (_, health))| health.is_available(model))
        .map(|(i, _)| i)
        .collect();

    if eligible.is_empty() {
        return Err(RetryFailure::bare(UpstreamError::NoEligibleCredentials));
    }

    // If a specific credential is forced (file affinity), try it first
    let mut remaining = if let Some(forced) = forced_credential
        && eligible.contains(&forced)
    {
        let mut v = vec![forced];
        v.extend(eligible.iter().filter(|&&i| i != forced));
        v
    } else {
        build_remaining_candidates(&eligible, round_robin_cursor, affinity_hint.is_some())
    };
    let mut last_error = None;
    // Diagnostics for the most recent attempt that produced a usable
    // upstream response (or a locally-raised error) — returned in
    // `RetryFailure.last_attempt` so the error-path logger can persist the
    // real upstream URL / headers / body / response body instead of a
    // placeholder row.
    let mut last_failed_attempt: Option<crate::response::FailedUpstreamAttempt> = None;

    while !remaining.is_empty() {
        let (remaining_idx, matched_affinity_idx) =
            pick_candidate_index(&remaining, affinity_hint, affinity_pool);
        let idx = remaining.remove(remaining_idx);
        tracing::info!(credential = idx, "trying credential");
        let mut attempts = 0u32;

        loop {
            let (credential, _) = &credentials[idx];

            // Select client: spoof for cookie-based credentials, normal otherwise
            let active_client = if channel.needs_spoof_client(credential) {
                spoof_client.unwrap_or(http_client)
            } else {
                http_client
            };

            // Build HTTP request
            let http_request = match channel.prepare_request(credential, settings, request) {
                Ok(req) => req,
                Err(e) => {
                    tracing::warn!(credential = idx, error = %e, "failed to prepare request");
                    last_failed_attempt = Some(crate::response::FailedUpstreamAttempt {
                        credential_index: Some(idx),
                        ..Default::default()
                    });
                    last_error = Some(e);
                    break;
                }
            };

            // Snapshot wire-level metadata for the upstream-request log.
            // Done before `send` because `http_request` is consumed there.
            let attempt_meta = UpstreamAttemptMeta {
                method: http_request.method().as_str().to_string(),
                url: http_request.uri().to_string(),
                request_headers: http_request
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                request_body: Some(http_request.body().clone()),
            };

            // Send request
            let method = attempt_meta.method.clone();
            let uri = attempt_meta.url.clone();
            tracing::info!(
                credential = idx,
                attempt = attempts,
                %method,
                %uri,
                model = model.unwrap_or(""),
                "sending upstream request"
            );
            let send_start = std::time::Instant::now();
            let raw_response = match send(active_client, http_request).await {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!(credential = idx, %method, %uri, error = %e, "upstream request failed");
                    last_failed_attempt = Some(crate::response::FailedUpstreamAttempt {
                        method: attempt_meta.method.clone(),
                        url: attempt_meta.url.clone(),
                        request_headers: attempt_meta.request_headers.clone(),
                        request_body: attempt_meta.request_body.clone(),
                        credential_index: Some(idx),
                        ..Default::default()
                    });
                    last_error = Some(e);
                    break;
                }
            };

            let latency_ms = send_start.elapsed().as_millis() as u64;

            // Determine if this is an immediate success (streaming 2xx) or needs classification
            let response = match raw_response.into_retry_action() {
                RetryAction::ImmediateSuccess { status, output } => {
                    tracing::info!(
                        credential = idx,
                        status,
                        latency_ms,
                        "upstream response received"
                    );
                    let (_, health) = &mut credentials[idx];
                    health.record_success(model);
                    bind_affinity(affinity_pool, affinity_hint, idx, matched_affinity_idx);
                    return Ok(RetryResult {
                        output,
                        credential_index: idx,
                        attempt_meta,
                    });
                }
                RetryAction::Classifiable(resp) => resp,
            };

            // Classify buffered response
            tracing::info!(
                credential = idx,
                status = response.status,
                latency_ms,
                "upstream response received"
            );
            let classification =
                channel.classify_response(response.status, &response.headers, &response.body);

            // Snapshot the response side of the failed-attempt diagnostics.
            // Cloned up front so each error branch can record it before any
            // further mutation of `response`. Cheap on the failure path
            // (a few hundred bytes typically); skipped entirely on Success
            // because that branch returns before this value is consumed.
            let make_failed_attempt = || crate::response::FailedUpstreamAttempt {
                method: attempt_meta.method.clone(),
                url: attempt_meta.url.clone(),
                request_headers: attempt_meta.request_headers.clone(),
                request_body: attempt_meta.request_body.clone(),
                response_status: Some(response.status),
                response_headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                response_body: Some(response.body.clone()),
                credential_index: Some(idx),
            };

            let (_, health) = &mut credentials[idx];
            match classification {
                ResponseClassification::Success => {
                    health.record_success(model);
                    bind_affinity(affinity_pool, affinity_hint, idx, matched_affinity_idx);
                    return Ok(RetryResult {
                        output: R::wrap_buffered(response),
                        credential_index: idx,
                        attempt_meta,
                    });
                }
                ResponseClassification::AuthDead => {
                    tracing::warn!(
                        credential = idx,
                        status = response.status,
                        model = model.unwrap_or(""),
                        "credential auth dead, attempting refresh"
                    );
                    last_failed_attempt = Some(make_failed_attempt());
                    let (credential, health) = &mut credentials[idx];
                    let refreshed = channel
                        .refresh_credential(http_client, credential)
                        .await
                        .unwrap_or(false);

                    if refreshed {
                        let retry_request = match channel
                            .prepare_request(credential, settings, request)
                        {
                            Ok(req) => req,
                            Err(e) => {
                                tracing::warn!(credential = idx, error = %e, "failed to prepare request after refresh");
                                last_error = Some(e);
                                break;
                            }
                        };

                        // Snapshot the refreshed-attempt wire metadata for logging.
                        let refresh_meta = UpstreamAttemptMeta {
                            method: retry_request.method().as_str().to_string(),
                            url: retry_request.uri().to_string(),
                            request_headers: retry_request
                                .headers()
                                .iter()
                                .map(|(k, v)| {
                                    (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())
                                })
                                .collect(),
                            request_body: Some(retry_request.body().clone()),
                        };

                        match send(active_client, retry_request).await {
                            Ok(raw_retry) => match raw_retry.into_retry_action() {
                                RetryAction::ImmediateSuccess { output, .. } => {
                                    health.record_success(model);
                                    bind_affinity(
                                        affinity_pool,
                                        affinity_hint,
                                        idx,
                                        matched_affinity_idx,
                                    );
                                    return Ok(RetryResult {
                                        output,
                                        credential_index: idx,
                                        attempt_meta: refresh_meta,
                                    });
                                }
                                RetryAction::Classifiable(retry_response) => {
                                    let retry_class = channel.classify_response(
                                        retry_response.status,
                                        &retry_response.headers,
                                        &retry_response.body,
                                    );
                                    if matches!(retry_class, ResponseClassification::Success) {
                                        health.record_success(model);
                                        bind_affinity(
                                            affinity_pool,
                                            affinity_hint,
                                            idx,
                                            matched_affinity_idx,
                                        );
                                        return Ok(RetryResult {
                                            output: R::wrap_buffered(retry_response),
                                            credential_index: idx,
                                            attempt_meta: refresh_meta,
                                        });
                                    }
                                    health.record_error(retry_response.status, model, None);
                                    tracing::warn!(
                                        credential = idx,
                                        status = retry_response.status,
                                        "credential still dead after refresh"
                                    );
                                    // Overwrite with the post-refresh attempt
                                    // so the log reflects the final state.
                                    last_failed_attempt =
                                        Some(crate::response::FailedUpstreamAttempt {
                                            method: refresh_meta.method.clone(),
                                            url: refresh_meta.url.clone(),
                                            request_headers: refresh_meta.request_headers.clone(),
                                            request_body: refresh_meta.request_body.clone(),
                                            response_status: Some(retry_response.status),
                                            response_headers: retry_response
                                                .headers
                                                .iter()
                                                .map(|(k, v)| {
                                                    (
                                                        k.to_string(),
                                                        v.to_str().unwrap_or("").to_string(),
                                                    )
                                                })
                                                .collect(),
                                            response_body: Some(retry_response.body.clone()),
                                            credential_index: Some(idx),
                                        });
                                }
                            },
                            Err(e) => {
                                tracing::warn!(credential = idx, error = %e, "upstream request failed after refresh");
                                last_error = Some(e);
                            }
                        }
                    } else {
                        health.record_error(response.status, model, None);
                        tracing::warn!(
                            credential = idx,
                            status = response.status,
                            "credential auth dead, refresh not available"
                        );
                    }
                    clear_affinity(affinity_pool, affinity_hint, matched_affinity_idx);
                    break;
                }
                ResponseClassification::RateLimited { retry_after_ms } => {
                    if retry_after_ms.is_some() {
                        health.record_error(response.status, model, retry_after_ms);
                        tracing::warn!(
                            credential = idx,
                            status = response.status,
                            retry_after_ms = retry_after_ms.unwrap_or(0),
                            model = model.unwrap_or(""),
                            "rate limited with retry-after, switching credential"
                        );
                        last_failed_attempt = Some(make_failed_attempt());
                        clear_affinity(affinity_pool, affinity_hint, matched_affinity_idx);
                        break;
                    }
                    attempts += 1;
                    if attempts >= max_retries {
                        health.record_error(response.status, model, None);
                        tracing::warn!(
                            credential = idx,
                            status = response.status,
                            attempts,
                            max_retries,
                            model = model.unwrap_or(""),
                            "rate limited, retries exhausted"
                        );
                        last_failed_attempt = Some(make_failed_attempt());
                        clear_affinity(affinity_pool, affinity_hint, matched_affinity_idx);
                        break;
                    }
                    tracing::info!(
                        credential = idx,
                        status = response.status,
                        attempt = attempts,
                        max_retries,
                        "rate limited without retry-after, retrying"
                    );
                    continue;
                }
                ResponseClassification::TransientError => {
                    health.record_error(response.status, model, None);
                    tracing::warn!(
                        credential = idx,
                        status = response.status,
                        model = model.unwrap_or(""),
                        "transient error"
                    );
                    last_failed_attempt = Some(make_failed_attempt());
                    clear_affinity(affinity_pool, affinity_hint, matched_affinity_idx);
                    break;
                }
                ResponseClassification::PermanentError => {
                    return Ok(RetryResult {
                        output: R::wrap_buffered(response),
                        credential_index: idx,
                        attempt_meta,
                    });
                }
            }
        }
    }

    Err(RetryFailure {
        error: last_error.unwrap_or(UpstreamError::AllCredentialsExhausted),
        last_attempt: last_failed_attempt,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bind_affinity(
    pool: &CacheAffinityPool,
    hint: Option<&CacheAffinityHint>,
    idx: usize,
    matched_affinity_idx: Option<usize>,
) {
    if let Some(hint) = hint {
        pool.bind(&hint.bind.key, idx, hint.bind.ttl_ms);
        if let Some(matched_idx) = matched_affinity_idx
            && let Some(hit) = hint.candidates.get(matched_idx)
        {
            pool.bind(&hit.key, idx, hit.ttl_ms);
        }
    }
}

fn clear_affinity(
    pool: &CacheAffinityPool,
    hint: Option<&CacheAffinityHint>,
    matched_affinity_idx: Option<usize>,
) {
    if let Some(matched_idx) = matched_affinity_idx
        && let Some(hint) = hint
        && let Some(hit) = hint.candidates.get(matched_idx)
    {
        pool.clear(&hit.key);
    }
}

fn build_remaining_candidates(
    eligible: &[usize],
    round_robin_cursor: &AtomicUsize,
    use_random: bool,
) -> Vec<usize> {
    if eligible.is_empty() {
        return Vec::new();
    }

    if use_random {
        // Random order: cache affinity will steer to the right credential,
        // and random base order prevents sequential bias that undermines affinity.
        use rand::seq::SliceRandom;
        let mut candidates: Vec<usize> = eligible.to_vec();
        candidates.shuffle(&mut rand::rng());
        candidates
    } else {
        // Round-robin: deterministic rotation across credentials.
        let start = round_robin_cursor.fetch_add(1, Ordering::Relaxed) % eligible.len();
        (0..eligible.len())
            .map(|offset| eligible[(start + offset) % eligible.len()])
            .collect()
    }
}

fn pick_candidate_index(
    remaining: &[usize],
    affinity_hint: Option<&CacheAffinityHint>,
    affinity_pool: &CacheAffinityPool,
) -> (usize, Option<usize>) {
    let Some(hint) = affinity_hint else {
        return (0, None);
    };

    let remaining_idx_by_credential = remaining
        .iter()
        .enumerate()
        .map(|(idx, credential_idx)| (*credential_idx, idx))
        .collect::<HashMap<_, _>>();
    let mut score_by_credential = HashMap::<usize, usize>::new();
    let mut representative_match = HashMap::<usize, (usize, usize)>::new();

    for (candidate_idx, candidate) in hint.candidates.iter().enumerate() {
        let Some(credential_idx) = affinity_pool.get(&candidate.key) else {
            continue;
        };
        if !remaining_idx_by_credential.contains_key(&credential_idx) {
            continue;
        }

        let score = score_by_credential.entry(credential_idx).or_default();
        *score = score.saturating_add(candidate.key_len);

        representative_match
            .entry(credential_idx)
            .and_modify(|(best_idx, best_len)| {
                if candidate.key_len > *best_len {
                    *best_idx = candidate_idx;
                    *best_len = candidate.key_len;
                }
            })
            .or_insert((candidate_idx, candidate.key_len));
    }

    let mut best: Option<(usize, usize, usize)> = None;
    for (credential_idx, score) in score_by_credential {
        let Some(&remaining_idx) = remaining_idx_by_credential.get(&credential_idx) else {
            continue;
        };
        let matched_idx = representative_match
            .get(&credential_idx)
            .map(|(idx, _)| *idx)
            .unwrap_or_default();

        match best {
            None => best = Some((remaining_idx, score, matched_idx)),
            Some((best_remaining_idx, best_score, _)) => {
                if score > best_score || (score == best_score && remaining_idx < best_remaining_idx)
                {
                    best = Some((remaining_idx, score, matched_idx));
                }
            }
        }
    }

    if let Some((remaining_idx, _, matched_idx)) = best {
        (remaining_idx, Some(matched_idx))
    } else {
        (0, None)
    }
}
