use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};

#[cfg(not(target_arch = "wasm32"))]
use super::{auth, config::PlaygroundConfig};
use super::{auth::Session, config, stream, wire};
use crate::channel::{ChannelError, OperationContext, read_body};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};

pub(super) fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("Playground: {}", message.into()))
}

pub(super) fn post(
    url: &str,
    headers: HeaderMap,
    body: &Value,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let mut builder = http::Request::builder().method("POST").uri(url);
    *builder
        .headers_mut()
        .ok_or_else(|| invalid("invalid RPC URL"))? = headers;
    builder
        .body(HttpBody::Bytes(body.to_string().into()))
        .map_err(|_| ChannelError::InvalidConfig("invalid Playground endpoint".into()))
}

fn require_gemini(
    context: &OperationContext<'_>,
    operation: Operation,
) -> Result<(), ChannelError> {
    if context.dialect == Dialect::Gemini {
        Ok(())
    } else {
        Err(ChannelError::UnsupportedOperation(OperationKey {
            operation,
            dialect: context.dialect,
        }))
    }
}

fn model(path: &str) -> Result<String, ChannelError> {
    let (_, model) = path
        .split_once("models/")
        .ok_or_else(|| invalid("missing model in request path"))?;
    let model = model
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    if model.is_empty() {
        return Err(invalid("empty model"));
    }
    Ok(format!("models/{model}"))
}

fn json_response(status: StatusCode, body: Value) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body.to_string().into()),
    }
}

pub(super) async fn error_response(
    response: WireResponse<HttpBody>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let bytes = read_body(response.body).await?;
    let body = stream::parse_jspb(&bytes).ok();
    let status = body.as_ref().and_then(|v| {
        if v.get(0).is_some_and(Value::is_number) {
            Some(v)
        } else {
            v.get(1).filter(|v| v.get(0).is_some_and(Value::is_number))
        }
    });
    let message = status
        .and_then(|v| v.get(1))
        .and_then(Value::as_str)
        .unwrap_or("Google AI Studio rejected the request");
    let reason = match status.and_then(|v| v.get(0)).and_then(Value::as_u64) {
        Some(3) => "INVALID_ARGUMENT",
        Some(5) => "NOT_FOUND",
        Some(7) => "PERMISSION_DENIED",
        Some(8) => "RESOURCE_EXHAUSTED",
        Some(16) => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    };
    let mut shaped = json_response(
        response.status,
        json!({"error":{"code":response.status.as_u16(),"message":message,"status":reason}}),
    );
    let mut headers = response.headers;
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_ENCODING,
        header::SET_COOKIE,
    ] {
        headers.remove(name);
    }
    // In particular, retain Retry-After for the host's credential cooldown.
    shaped.headers.extend(headers);
    Ok(shaped)
}

pub(super) async fn models(
    context: OperationContext<'_>,
    single: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    require_gemini(
        &context,
        if single {
            Operation::GetModel
        } else {
            Operation::ListModels
        },
    )?;
    let session = Session::parse(context.credential.secret)?;
    let response = context
        .client
        .send(post(
            &config::endpoint(context.provider, "ListModels", context.endpoint_override),
            session.headers(None)?,
            &json!([]),
        )?)
        .await?;
    if !response.status.is_success() {
        return error_response(response).await;
    }
    let value = stream::parse_jspb(&read_body(response.body).await?)?;
    let models = wire::models(&value)?;
    if single {
        let name = model(&context.request.path)?;
        return Ok(
            match models.into_iter().find(|value| value["name"] == name) {
                Some(value) => json_response(StatusCode::OK, value),
                None => json_response(
                    StatusCode::NOT_FOUND,
                    json!({"error":{"code":404,"status":"NOT_FOUND","message":"Model is not available in AI Studio Playground"}}),
                ),
            },
        );
    }
    Ok(json_response(StatusCode::OK, json!({"models":models})))
}

pub(super) async fn generate(
    context: OperationContext<'_>,
    streaming: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    require_gemini(
        &context,
        if streaming {
            Operation::StreamGenerateContent
        } else {
            Operation::GenerateContent
        },
    )?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (context, streaming);
        Err(invalid(
            "Playground is unavailable on this build: its WAA proof signer embeds QuickJS, which does not run on the edge/wasm32 target",
        ))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        generate_native(context, streaming).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn generate_native(
    context: OperationContext<'_>,
    streaming: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    use super::{signer, waa};
    let config = PlaygroundConfig::parse(context.provider)?;
    let session = Session::parse(context.credential.secret)?;
    let model = model(&context.request.path)?;
    let model_short = model.strip_prefix("models/").unwrap_or(&model).to_string();
    let body: Value = serde_json::from_slice(&read_body(context.request.body).await?)
        .map_err(|_| ChannelError::InvalidConfig("Playground request must be JSON".into()))?;
    let mut wire = wire::encode(&model, &body, &config.timezone)?;
    let tier = context
        .credential
        .metadata
        .get("benefit_tier")
        .and_then(Value::as_str);

    // Mint the proof in-process on the QuickJS-hosted Firefox DOM, on this
    // session's live signer when there is one. No external signing service.
    let fallback = wire::prompt(&wire);
    let (binding, page) = waa::extract_prompts(wire.get(1).unwrap_or(&Value::Null), &fallback);
    let minted = signer::proof(
        context.client.as_ref(),
        &session,
        tier,
        &model_short,
        binding,
        page,
    )
    .await?;
    wire[4] = minted.into();

    let mut headers = session.headers(tier)?;
    // The proof attests a Firefox environment; the GenerateContent request must
    // carry Firefox's UA (session headers default to the channel's Chrome UA).
    headers.insert(header::USER_AGENT, auth::header_value(waa::FIREFOX_UA)?);

    let mut response = context
        .client
        .send(post(
            &config::endpoint(
                context.provider,
                "GenerateContent",
                context.endpoint_override,
            ),
            headers,
            &wire,
        )?)
        .await?;
    if !response.status.is_success() {
        // A refused request may mean the signer's session went stale; 429 and 5xx
        // are quota and capacity, which a new session does not change.
        if response.status.is_client_error() && response.status != StatusCode::TOO_MANY_REQUESTS {
            signer::discard(&session, &model_short);
        }
        return error_response(response).await;
    }
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.remove(header::CONTENT_ENCODING);
    response.headers.remove(header::SET_COOKIE);
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(if streaming {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    response.body = if streaming {
        HttpBody::Stream(stream::translate(response.body, model))
    } else {
        HttpBody::Bytes(
            stream::collect(response.body, &model)
                .await?
                .to_string()
                .into(),
        )
    };
    Ok(response)
}

/// Call Waa/Create with the session's auth but the WAA API key and Firefox UA.
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn fetch_create(
    client: &dyn gproxy_client::OutboundClient,
    session: &Session,
    tier: Option<&str>,
) -> Result<super::waa::Challenge, ChannelError> {
    use super::waa;
    let mut headers = session.headers(tier)?;
    headers.insert("x-goog-api-key", auth::header_value(waa::WAA_API_KEY)?);
    headers.insert(header::USER_AGENT, auth::header_value(waa::FIREFOX_UA)?);
    let response = client
        .send(post(waa::WAA_CREATE_URL, headers, &json!([waa::WAA_REQUEST_KEY]))?)
        .await?;
    if !response.status.is_success() {
        return Err(invalid(format!(
            "Waa/Create returned HTTP {}",
            response.status.as_u16()
        )));
    }
    let raw = read_body(response.body).await?;
    waa::parse_challenge(&raw).map_err(|e| invalid(e.to_string()))
}

/// The interpreter JavaScript at `https://www.google.com/js/bg/<hash>.js`. The
/// URL names its content by hash and changes only every few days, so the text is
/// kept for the process and fetched once per URL.
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn fetch_interpreter(
    client: &dyn gproxy_client::OutboundClient,
    url: &str,
) -> Result<String, ChannelError> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    /// Old interpreters are not asked for again; a few entries cover a rotation.
    const KEPT: usize = 4;
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(text) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(url) {
        return Ok(text.clone());
    }
    let text = fetch_text(client, url).await?;
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= KEPT {
        cache.clear();
    }
    cache.insert(url.to_string(), text.clone());
    Ok(text)
}

/// GET the interpreter JavaScript from `https://www.google.com/js/bg/<hash>.js`.
#[cfg(not(target_arch = "wasm32"))]
async fn fetch_text(
    client: &dyn gproxy_client::OutboundClient,
    url: &str,
) -> Result<String, ChannelError> {
    let request = http::Request::builder()
        .method("GET")
        .uri(url)
        .header(header::USER_AGENT, super::waa::FIREFOX_UA)
        .body(HttpBody::Bytes(Default::default()))
        .map_err(|_| invalid("invalid interpreter URL"))?;
    let response = client.send(request).await?;
    if !response.status.is_success() {
        return Err(invalid(format!(
            "interpreter fetch returned HTTP {}",
            response.status.as_u16()
        )));
    }
    let raw = read_body(response.body).await?;
    String::from_utf8(raw.to_vec()).map_err(|_| invalid("interpreter not UTF-8"))
}
