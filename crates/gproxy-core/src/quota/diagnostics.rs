//! Response capture for an explicitly requested quota diagnostic. Nothing is persisted.
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use gproxy_client::OutboundClient;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture},
};
use serde_json::{Value, json};

pub(super) type Responses = Arc<Mutex<Vec<Value>>>;

pub(super) struct CaptureClient<'a> {
    pub inner: &'a dyn OutboundClient,
    pub responses: Responses,
    pub secrets: Vec<String>,
}

impl OutboundClient for CaptureClient<'_> {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let mut response = self.inner.send(request).await?;
            // Quota replies are already consumed as complete documents by the channels.
            let bytes = match response.body {
                HttpBody::Bytes(bytes) => bytes,
                HttpBody::Stream(mut stream) => {
                    let mut bytes = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        bytes.extend_from_slice(&chunk.map_err(|error| {
                            CapabilityError::with_source(
                                CapabilityErrorKind::Transport,
                                CapabilityErrorStage::BodyTransfer,
                                "quota response body failed",
                                error,
                            )
                        })?);
                    }
                    bytes.into()
                }
            };
            let mut body = serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
            redact(&mut body, &self.secrets);
            self.responses
                .lock()
                .expect("quota diagnostics lock")
                .push(json!({
                    "status": response.status.as_u16(), "body": body,
                }));
            response.body = HttpBody::Bytes(bytes);
            Ok(response)
        })
    }
}

fn sensitive(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace(['_', '-'], "").as_str(),
        "authorization"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "key"
            | "password"
            | "secret"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "sessiontoken"
            | "email"
            | "userid"
            | "accountid"
            | "organizationid"
    )
}

pub(super) fn secrets(value: &Value) -> Vec<String> {
    match value {
        Value::Object(values) => values
            .iter()
            .flat_map(|(key, value)| {
                if sensitive(key)
                    && let Some(value) = value.as_str().filter(|v| !v.is_empty())
                {
                    vec![value.to_owned()]
                } else {
                    secrets(value)
                }
            })
            .collect(),
        Value::Array(values) => values.iter().flat_map(secrets).collect(),
        _ => Vec::new(),
    }
}

fn redact(value: &mut Value, secrets: &[String]) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if sensitive(key) {
                    *value = Value::String("[redacted]".into());
                } else {
                    redact(value, secrets);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact(value, secrets);
            }
        }
        Value::String(text) => {
            for secret in secrets {
                *text = text.replace(secret, "[redacted]");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_protocol::connection::Bytes;

    struct Reply(bool);
    impl OutboundClient for Reply {
        fn send<'a>(
            &'a self,
            _: http::Request<HttpBody>,
        ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
            Box::pin(async move {
                let bytes = Bytes::from_static(br#"{"access_token":"secret-123","email":"person@example.test","output_tokens":"100","details":{"message":"secret-123 expired","limit":20}}"#);
                Ok(WireResponse {
                    status: http::StatusCode::TOO_MANY_REQUESTS,
                    headers: Default::default(),
                    body: if self.0 {
                        HttpBody::Stream(Box::pin(futures_util::stream::once(async { Ok(bytes) })))
                    } else {
                        HttpBody::Bytes(bytes)
                    },
                })
            })
        }
    }

    #[tokio::test]
    async fn failed_responses_are_redacted_without_changing_the_channel_body() {
        for streamed in [false, true] {
            let responses = Responses::default();
            let client = CaptureClient {
                inner: &Reply(streamed),
                responses: responses.clone(),
                secrets: secrets(&json!({"access_token":"secret-123"})),
            };
            let response = client
                .send(http::Request::new(HttpBody::Bytes(Bytes::new())))
                .await
                .unwrap();
            let HttpBody::Bytes(body) = response.body else {
                panic!("buffered quota body")
            };
            assert!(String::from_utf8_lossy(&body).contains("secret-123"));
            let captured = responses.lock().unwrap();
            assert_eq!(captured[0]["status"], 429);
            assert_eq!(captured[0]["body"]["output_tokens"], "100");
            assert_eq!(captured[0]["body"]["details"]["limit"], 20);
            assert!(!captured[0].to_string().contains("secret-123"));
            assert!(!captured[0].to_string().contains("person@example.test"));
        }
    }
}
