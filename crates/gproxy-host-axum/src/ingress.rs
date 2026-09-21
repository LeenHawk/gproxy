//! What a request path means, in the one order it may be decided in.
//!
//! Everything that is not one of this gateway's own routes lands here, and
//! this module is the whole of the decision. The order is not arbitrary and is
//! worth reading as a list of precedences rather than as a pipeline:
//!
//! 1. **CORS and the client address.** A preflight is answered and never
//!    forwarded. The client address is resolved under the trusted-proxy rule
//!    ([`crate::policy`]) before anything can log it.
//! 2. **The mount** ([`Mount::parse`]). Which slice of the path is this
//!    gateway's and which belongs to the API being forwarded.
//! 3. **The OAuth issuer**, at `{mount}/v1/oauth/…`. Before the data plane
//!    because these paths are *this* instance's, not an upstream's, and they
//!    answer in a different error envelope.
//! 4. **A channel's vendor service route**, e.g. Codex's `/backend-api/…`.
//!    Only on a provider or namespace mount: matching a service route needs a
//!    channel, and the aggregated mount names none.
//! 5. **The data plane** — `/v1/messages`, `/v1/chat/completions`,
//!    `/v1beta/models/{model}:generateContent` and the rest of
//!    [`surface`]. The body is decoded, the operation is matched, and
//!    [`App::call`](gproxy_app::App::call) does the rest.
//! 6. **The console**, with its SPA fallback, behind the configuration switch.
//!
//! Anything that reaches the end is a 404.
//!
//! # A mount prefixes the model name
//!
//! A provider or namespace mount narrows resolution by rewriting the model the
//! caller asked for: `/acme/v1/messages` with `{"model":"fast"}` resolves
//! `acme/fast`. That is the sdk resolver's own grammar for "this provider" and
//! "this route", so the host adds no rule of its own — it only says which
//! prefix the URL asked for. A name that already carries the prefix is left
//! alone, so a client may spell it either way.
//!
//! A provider mount additionally restricts the channel, which is what narrows
//! the operations that name no model at all (listing models, for instance).
//! Narrowing those to the single provider needs a field `DataPlaneRequest`
//! does not have yet; see the crate README.

pub mod surface;

use std::sync::Arc;

use axum::{
    body::to_bytes,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use gproxy_app::{App, AppError, Caller, DataPlaneRequest, RequestedView, ServiceRequestIn};
use gproxy_channel::channel::{ServiceRoute, ServiceTransport};
use gproxy_protocol::connection::Bytes;
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};

use crate::{
    HostState, Mount,
    error::ErrorResponse,
    mount::MountIndex,
    policy,
    response::{passthrough_wire, streamed},
};

/// The header a client names a vendor service view with. Absent means
/// [`RequestedView::Caller`], which is the only view an ordinary member may
/// ask for anyway.
pub const VIEW_HEADER: &str = "x-gproxy-view";

/// The fallback handler: everything that is not one of this host's own routes.
pub async fn handle<C>(State(state): State<HostState<C>>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let origins = &state.app().config().cors_origins;
    let origin = policy::allowed_origin(request.headers(), origins);
    if policy::is_preflight(request.method(), request.headers()) {
        // A preflight is never forwarded: it asks this instance what it will
        // accept, and the upstream has no opinion about that.
        return policy::apply_preflight_cors(
            StatusCode::NO_CONTENT.into_response(),
            origin.as_ref(),
            request.headers(),
        );
    }
    let response = dispatch(&state, request).await;
    policy::apply_cors(response, origin.as_ref())
}

async fn dispatch<C>(state: &HostState<C>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let peer = crate::peer_ip(&request);
    let trusted = &state.app().config().trusted_proxies;
    let client_ip = policy::client_ip(peer, request.headers(), trusted).to_string();
    let scheme = policy::client_scheme(peer, request.headers(), trusted);

    let (mut parts, body) = request.into_parts();
    let body = match to_bytes(body, crate::MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };

    // One load of each snapshot for the whole request, as everywhere else.
    let routing = state.app().gproxy().routing();
    let core = state.app().gproxy().core().snapshot();
    let index = MountIndex::build(&routing, &core);
    let path = parts.uri.path().to_owned();
    let (mount, remainder) = Mount::parse(&path, &index, |remainder| {
        surface::is_surface(&parts.method, remainder)
            || crate::oauth::is_issuer_path(remainder)
            || core
                .providers
                .values()
                .any(|provider| service_route(provider, &parts.method, remainder).is_some())
    });

    if let Some(issuer) = crate::oauth::route(&parts.method, &remainder) {
        return crate::oauth::handle(state, issuer, &mount, scheme, parts, body).await;
    }

    if let Some(response) =
        service_call(state, &core, &mount, &index, &remainder, &parts, &body).await
    {
        return response;
    }

    if let Some(response) = data_plane(
        state, &mount, &core, &index, &remainder, &mut parts, body, &client_ip,
    )
    .await
    {
        return response;
    }

    if let Some(response) = state.console().serve(&parts.method, &path).await {
        return response;
    }

    ErrorResponse(AppError::not_found("route", path)).into_response()
}

/// The service route of one provider's channel that matches this request.
fn service_route<'a>(
    provider: &'a gproxy_core::ProviderData,
    method: &Method,
    path: &str,
) -> Option<&'a ServiceRoute> {
    provider
        .channel
        .services()?
        .routes()
        .iter()
        .find(|route| route.matches(method, path).is_some())
}

/// Step 4: a vendor service the channel declares, forwarded as it is.
///
/// Only on a mount that names a provider or a namespace, because matching a
/// route needs a channel and the aggregated mount names none. A namespace
/// narrows to the providers its exposed names reach; the first that declares a
/// matching route serves it, which is deterministic because `CoreData` orders
/// providers by id.
async fn service_call<C>(
    state: &HostState<C>,
    core: &gproxy_core::CoreData,
    mount: &Mount,
    index: &MountIndex,
    remainder: &str,
    parts: &http::request::Parts,
    body: &Bytes,
) -> Option<Response>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let provider_id = match mount {
        Mount::Aggregated => return None,
        Mount::Provider(name) => index.provider_id(name)?.to_owned(),
        // A namespace can reach several providers; the one that declares this
        // route serves it.
        Mount::Namespace(_) => core
            .providers
            .iter()
            .find(|(_, provider)| service_route(provider, &parts.method, remainder).is_some())
            .map(|(id, _)| id.clone())?,
    };
    let route = service_route(core.providers.get(&provider_id)?, &parts.method, remainder)?;
    if route.transport == ServiceTransport::WebSocket {
        // P10 owns the upgrade. Until then a client is told plainly rather
        // than handed a buffered body it cannot use.
        return Some(upgrade_required("this vendor service is a websocket"));
    }

    let caller = match authenticate(state.app(), parts).await {
        Ok(caller) => caller,
        Err(error) => return Some(ErrorResponse(error).into_response()),
    };
    let view = match requested_view(&parts.headers) {
        Ok(view) => view,
        Err(error) => return Some(ErrorResponse(error).into_response()),
    };
    let mut parts = parts.clone();
    // The channel matches its own routes against the path, so it is the
    // mount-relative one that is forwarded, not the one the client typed.
    parts.uri = rewrite_path(&parts.uri, remainder);
    let request = ServiceRequestIn {
        request_id: crate::request_id(),
        parts,
        body: body.clone(),
        view,
        provider_id,
    };
    Some(match state.app().call_service(&caller, request).await {
        Ok(response) => passthrough_wire(response),
        Err(error) => ErrorResponse(error).into_response(),
    })
}

/// Step 5: the model API.
///
/// `None` when the path is not one of the declared surfaces, which is what
/// lets the console have the last word on a path that is neither.
#[allow(clippy::too_many_arguments)]
async fn data_plane<C>(
    state: &HostState<C>,
    mount: &Mount,
    core: &gproxy_core::CoreData,
    index: &MountIndex,
    remainder: &str,
    parts: &mut http::request::Parts,
    body: Bytes,
    client_ip: &str,
) -> Option<Response>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let body = match decode_body(&mut parts.headers, body, crate::MAX_BODY_BYTES) {
        Ok(body) => body,
        Err(error) => return Some((error.status, error.message).into_response()),
    };
    let json: Option<serde_json::Value> = (!body.is_empty())
        .then(|| serde_json::from_slice(&body).ok())
        .flatten();
    let matched = surface::match_path(&parts.method, remainder, &parts.headers, json.as_ref())?;

    if matched.upgrade {
        // The handshake itself is P10's. The route is declared here so that
        // the mount grammar already accepts it and a client gets a clear
        // refusal rather than a 404 that looks like a missing model.
        return Some(upgrade_required(
            "websocket upgrades are not served by this build",
        ));
    }

    let caller = match authenticate(state.app(), parts).await {
        Ok(caller) => caller,
        Err(error) => return Some(ErrorResponse(error).into_response()),
    };

    let mut parts = parts.clone();
    parts.uri = rewrite_path(&parts.uri, remainder);
    let model = matched
        .model
        .clone()
        .or_else(|| body_model(json.as_ref()))
        .map(|model| mounted_model(mount, &model));
    let mut request = DataPlaneRequest::new(crate::request_id(), matched.operation, parts, body);
    request.client_ip = Some(client_ip.to_owned());
    request.model = model;
    // What narrows the operations that name no model. A provider mount is one
    // provider, and one provider is one channel.
    request.channel = mount
        .provider_name()
        .and_then(|name| index.provider_id(name))
        .and_then(|id| core.providers.get(id))
        .map(|provider| provider.channel.id().to_owned());

    let app = Arc::clone(state.app());
    let outcome = app.call(&caller, request).await;
    Some(match outcome {
        Ok(outcome) => streamed(app, outcome),
        Err(error) => ErrorResponse(error).into_response(),
    })
}

/// The model name as the mount asked for it.
///
/// `acme` + `fast` is `acme/fast`; `acme` + `acme/fast` is unchanged, so a
/// client may spell the name either way and a console that renders the exposed
/// name verbatim still works against a namespaced base URL.
fn mounted_model(mount: &Mount, model: &str) -> String {
    match mount {
        Mount::Aggregated => model.to_owned(),
        Mount::Namespace(name) | Mount::Provider(name) => {
            if model
                .strip_prefix(name.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
            {
                model.to_owned()
            } else {
                format!("{name}/{model}")
            }
        }
    }
}

fn body_model(json: Option<&serde_json::Value>) -> Option<String> {
    Some(json?.get("model")?.as_str()?.to_owned())
}

/// The data-plane credential ladder, plus the one thing `gproxy-app` refuses
/// to do for us.
///
/// [`Authenticator::authenticate_request`](gproxy_app::Authenticator::authenticate_request)
/// reads headers only and documents why: a key in the query string is the
/// transport's business and that crate has not parsed the URI. The Gemini
/// dialect puts one in `?key=`, so the host extracts it and presents it
/// explicitly — after the headers, so a header always wins.
async fn authenticate<C>(app: &App<C>, parts: &http::request::Parts) -> Result<Caller, AppError>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = app.data();
    let authenticator = app.authenticator(&data);
    match authenticator.authenticate_request(&parts.headers).await {
        Err(AppError::Unauthorized(reason)) => match query_key(parts.uri.query()) {
            Some(key) => authenticator.authenticate_token(&key).await,
            None => Err(AppError::Unauthorized(reason)),
        },
        other => other,
    }
}

/// Gemini's `?key=`. Percent-decoded, because a key travels through a query
/// string and `+` and `%xx` are that encoding's, not the key's.
fn query_key(query: Option<&str>) -> Option<String> {
    let pairs = serde_urlencoded::from_str::<Vec<(String, String)>>(query?).ok()?;
    pairs
        .into_iter()
        .find(|(name, _)| name == "key")
        .map(|(_, value)| value)
        .filter(|value| !value.trim().is_empty())
}

/// The vendor-service view the client asked for.
fn requested_view(headers: &HeaderMap) -> Result<RequestedView, AppError> {
    let Some(value) = headers.get(VIEW_HEADER) else {
        return Ok(RequestedView::Caller);
    };
    let value = value
        .to_str()
        .map_err(|_| AppError::invalid("the view header is not text"))?
        .trim();
    match value {
        "" | "caller" => Ok(RequestedView::Caller),
        "pool" => Ok(RequestedView::Pool),
        other => match other.split_once(':') {
            Some(("credential", id)) if !id.is_empty() => {
                Ok(RequestedView::Credential(id.to_owned()))
            }
            _ => Err(AppError::invalid(format!(
                "`{other}` is not a view; use `caller`, `pool` or `credential:{{id}}`"
            ))),
        },
    }
}

/// The URI with its path replaced by the mount-relative one, query intact.
///
/// An upstream is told the path of its own API, not the one this gateway is
/// mounted at, because a channel matches its routes against it and a provider
/// prefix means nothing there.
fn rewrite_path(uri: &http::Uri, path: &str) -> http::Uri {
    let target = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_owned(),
    };
    target.parse().unwrap_or_else(|_| uri.clone())
}

fn upgrade_required(message: &'static str) -> Response {
    let mut response = (StatusCode::UPGRADE_REQUIRED, message).into_response();
    response
        .headers_mut()
        .insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    response
}

/// A request body's content encoding, undone.
///
/// Ported from v3's `gproxy-app/src/ingress.rs` with its tests. `zstd` is the
/// one encoding decoded here, because the Codex CLI sends it and a channel
/// forwards bytes rather than framing; anything else is refused rather than
/// passed on, since an upstream that cannot read it would answer a confusing
/// 400 instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError {
    pub status: StatusCode,
    pub message: &'static str,
}

pub fn decode_body(
    headers: &mut HeaderMap,
    body: Bytes,
    max_bytes: usize,
) -> Result<Bytes, DecodeError> {
    use std::io::Read as _;

    let Some(encoding) = headers.get(header::CONTENT_ENCODING) else {
        return Ok(body);
    };
    let encoding = encoding.to_str().map_err(|_| unsupported_encoding())?;
    if encoding.eq_ignore_ascii_case("identity") {
        headers.remove(header::CONTENT_ENCODING);
        return Ok(body);
    }
    if !encoding.eq_ignore_ascii_case("zstd") {
        return Err(unsupported_encoding());
    }
    let decoder = ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(body))
        .map_err(|_| invalid_encoding())?;
    let mut decoded = Vec::with_capacity(max_bytes.min(64 * 1024));
    decoder
        // One byte past the limit, so a body that is exactly at it is accepted
        // and one that is over it is detected without decompressing the rest.
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| invalid_encoding())?;
    if decoded.len() > max_bytes {
        return Err(DecodeError {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            message: "decoded request body too large",
        });
    }
    headers.remove(header::CONTENT_ENCODING);
    headers.remove(header::CONTENT_LENGTH);
    Ok(Bytes::from(decoded))
}

fn unsupported_encoding() -> DecodeError {
    DecodeError {
        status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
        message: "unsupported content encoding",
    }
}

fn invalid_encoding() -> DecodeError {
    DecodeError {
        status: StatusCode::BAD_REQUEST,
        message: "invalid zstd request body",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruzstd::encoding::{CompressionLevel, compress_to_vec};

    #[test]
    fn decodes_zstd_and_removes_framing_headers() {
        let original = br#"{"model":"gpt-test"}"#;
        let compressed = compress_to_vec(original.as_slice(), CompressionLevel::Fastest);
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, "zstd".parse().unwrap());
        headers.insert(
            header::CONTENT_LENGTH,
            compressed.len().to_string().parse().unwrap(),
        );

        let decoded = decode_body(&mut headers, Bytes::from(compressed), 1024).unwrap();

        assert_eq!(decoded.as_ref(), original);
        assert!(!headers.contains_key(header::CONTENT_ENCODING));
        assert!(!headers.contains_key(header::CONTENT_LENGTH));
    }

    #[test]
    fn decoded_limit_and_unknown_encoding_fail_at_ingress() {
        let compressed = compress_to_vec([0_u8; 32].as_slice(), CompressionLevel::Fastest);
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, "zstd".parse().unwrap());
        assert_eq!(
            decode_body(&mut headers, Bytes::from(compressed), 16)
                .unwrap_err()
                .status,
            StatusCode::PAYLOAD_TOO_LARGE
        );

        headers.insert(header::CONTENT_ENCODING, "br".parse().unwrap());
        assert_eq!(
            decode_body(&mut headers, Bytes::new(), 16)
                .unwrap_err()
                .status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[test]
    fn a_mount_prefixes_the_model_once() {
        let namespace = Mount::Namespace("acme".into());
        assert_eq!(mounted_model(&namespace, "fast"), "acme/fast");
        assert_eq!(mounted_model(&namespace, "acme/fast"), "acme/fast");
        // A different name that merely starts with the same letters is not
        // the prefix.
        assert_eq!(mounted_model(&namespace, "acmex/fast"), "acme/acmex/fast");
        assert_eq!(mounted_model(&Mount::Aggregated, "fast"), "fast");
        assert_eq!(mounted_model(&Mount::Provider("p1".into()), "m1"), "p1/m1");
    }

    #[test]
    fn the_gemini_query_key_is_read_and_decoded() {
        assert_eq!(query_key(Some("key=sk-one")), Some("sk-one".to_owned()));
        assert_eq!(
            query_key(Some("alt=sse&key=sk%2Dtwo")),
            Some("sk-two".to_owned())
        );
        assert_eq!(query_key(Some("alt=sse")), None);
        assert_eq!(query_key(Some("key=")), None);
        assert_eq!(query_key(None), None);
    }

    #[test]
    fn a_view_is_named_or_refused() {
        let mut headers = HeaderMap::new();
        assert_eq!(requested_view(&headers).unwrap(), RequestedView::Caller);
        headers.insert(VIEW_HEADER, HeaderValue::from_static("pool"));
        assert_eq!(requested_view(&headers).unwrap(), RequestedView::Pool);
        headers.insert(VIEW_HEADER, HeaderValue::from_static("credential:c1"));
        assert_eq!(
            requested_view(&headers).unwrap(),
            RequestedView::Credential("c1".into())
        );
        headers.insert(VIEW_HEADER, HeaderValue::from_static("everything"));
        assert_eq!(requested_view(&headers).unwrap_err().status_code(), 400);
        headers.insert(VIEW_HEADER, HeaderValue::from_static("credential:"));
        assert_eq!(requested_view(&headers).unwrap_err().status_code(), 400);
    }

    #[test]
    fn the_upstream_is_told_its_own_path_with_the_query_intact() {
        let uri: http::Uri = "/acme/v1beta/models/m:generateContent?alt=sse"
            .parse()
            .unwrap();
        let rewritten = rewrite_path(&uri, "/v1beta/models/m:generateContent");
        assert_eq!(
            rewritten.to_string(),
            "/v1beta/models/m:generateContent?alt=sse"
        );
    }
}
