use bytes::Bytes;
use gproxy_channel_api::{ChannelError, SimpleHttp};
use http::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;

pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit i64")
}

pub(crate) fn form_request(
    method: http::Method,
    url: &str,
    fields: &[(&str, &str)],
) -> Result<http::Request<Bytes>, ChannelError> {
    http::Request::builder()
        .method(method)
        .uri(url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(super::http::form(fields)))
        .map_err(|error| ChannelError::Login(error.to_string()))
}

pub(crate) fn json_request(
    method: http::Method,
    url: &str,
    value: &Value,
) -> Result<http::Request<Bytes>, ChannelError> {
    http::Request::builder()
        .method(method)
        .uri(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(value.to_string()))
        .map_err(|error| ChannelError::Login(error.to_string()))
}

pub(crate) async fn send_json<T: serde::de::DeserializeOwned>(
    http: &dyn SimpleHttp,
    request: http::Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let response = http.send(request).await?;
    if !response.status().is_success() {
        let snippet: String = String::from_utf8_lossy(response.body())
            .chars()
            .take(256)
            .collect();
        return Err(ChannelError::Login(format!(
            "{what} endpoint {}: {snippet}",
            response.status()
        )));
    }
    parse_json(response.body(), what)
}

pub(crate) async fn send_json_any_status<T: serde::de::DeserializeOwned>(
    http: &dyn SimpleHttp,
    request: http::Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let response = http.send(request).await?;
    parse_json(response.body(), what)
}

fn parse_json<T: serde::de::DeserializeOwned>(body: &[u8], what: &str) -> Result<T, ChannelError> {
    serde_json::from_slice(body)
        .map_err(|error| ChannelError::Login(format!("{what} response JSON: {error}")))
}

#[cfg(test)]
pub(crate) mod test {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use bytes::Bytes;
    use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};

    pub(crate) struct MockHttp {
        responses: Mutex<VecDeque<http::Response<Bytes>>>,
        requests: Mutex<Vec<http::Request<Bytes>>>,
    }

    impl MockHttp {
        pub(crate) fn new(responses: &[(u16, &'static str)]) -> Self {
            Self {
                responses: Mutex::new(
                    responses
                        .iter()
                        .map(|(status, body)| {
                            http::Response::builder()
                                .status(*status)
                                .body(Bytes::from_static(body.as_bytes()))
                                .expect("mock response")
                        })
                        .collect(),
                ),
                requests: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn request_uris(&self) -> Vec<String> {
            self.requests
                .lock()
                .expect("mock requests")
                .iter()
                .map(|request| request.uri().to_string())
                .collect()
        }
    }

    impl SimpleHttp for MockHttp {
        fn send<'a>(
            &'a self,
            request: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
            self.requests.lock().expect("mock requests").push(request);
            let response = self.responses.lock().expect("mock responses").pop_front();
            Box::pin(async move {
                response.ok_or_else(|| ChannelError::Login("missing mock response".into()))
            })
        }
    }
}
