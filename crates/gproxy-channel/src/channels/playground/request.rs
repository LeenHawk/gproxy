use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};

use super::{
    auth::{self, Session},
    config::{self, PlaygroundConfig},
    stream, wire,
};
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
    let config = PlaygroundConfig::parse(context.provider)?;
    let proof_url = config.proof_url()?;
    let session = Session::parse(context.credential.secret)?;
    let model = model(&context.request.path)?;
    let body: Value = serde_json::from_slice(&read_body(context.request.body).await?)
        .map_err(|_| ChannelError::InvalidConfig("Playground request must be JSON".into()))?;
    let mut wire = wire::encode(&model, &body, &config.timezone)?;
    let tier = context
        .credential
        .metadata
        .get("benefit_tier")
        .and_then(Value::as_str);
    let mut signing_headers = HeaderMap::new();
    signing_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let signing_request = json!({
        "cookies":session.cookie(), "authUser":session.auth_user.parse::<u64>().map_err(|_| ChannelError::InvalidCredential)?,
        "g1Tier":tier.unwrap_or("BENEFIT_TIER_UNSPECIFIED"), "model":model,
        "prompt":wire::prompt(&wire), "wireContents":wire[1],
    });
    let signed = context
        .client
        .send(post(&proof_url, signing_headers, &signing_request)?)
        .await?;
    if !signed.status.is_success() {
        // The signing service's own authentication failure must not retire a
        // valid Google credential. Do not echo a service's secret-bearing body.
        return Err(invalid(format!(
            "WAA signing service returned HTTP {}",
            signed.status.as_u16()
        )));
    }
    let signed: Value = serde_json::from_slice(&read_body(signed.body).await?)
        .map_err(|_| invalid("invalid WAA signing response"))?;
    let proof = signed
        .get("proof")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .filter(|_| signed.get("ok") == Some(&Value::Bool(true)))
        .ok_or_else(|| invalid("WAA signing service did not return a proof"))?;
    wire[4] = proof.into();
    let dynamic = signed.get("headers").and_then(Value::as_object);
    // The signer can receive fresh Google session cookies while loading the
    // page. Use that same session for the proof-bound RPC, retaining the
    // selected Google identity rather than accepting a different authUser.
    let session = if let Some(cookie) = dynamic.and_then(|headers| {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("cookie"))
            .and_then(|(_, value)| value.as_str())
    }) {
        Session::parse(&json!({"cookie":cookie,"auth_user":session.auth_user}))?
    } else {
        session
    };
    let mut headers = session.headers(tier)?;
    // Only protocol identity returned by the selected signer is added. Neither
    // caller headers nor arbitrary signer fields may replace our authentication.
    if let Some(dynamic) = dynamic {
        for (name, value) in dynamic {
            let name = name.to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "user-agent" | "x-user-agent" | "x-goog-ext-519733851-bin" | "x-aistudio-visit-id"
            ) && let Some(value) = value.as_str()
            {
                headers.insert(
                    http::HeaderName::from_bytes(name.as_bytes())
                        .map_err(|_| invalid("invalid signer header"))?,
                    auth::header_value(value)?,
                );
            }
        }
    }
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
