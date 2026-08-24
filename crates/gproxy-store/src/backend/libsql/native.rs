use super::{HttpFuture, HttpSender};
use crate::StoreError;

pub(super) struct NativeSender {
    client: wreq::Client,
}

impl NativeSender {
    pub(super) fn new() -> Self {
        Self {
            client: wreq::Client::builder()
                .user_agent(concat!("gproxy/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("default libSQL HTTP client builds"),
        }
    }
}

impl HttpSender for NativeSender {
    fn post<'a>(&'a self, url: &'a str, auth_token: &'a str, body: Vec<u8>) -> HttpFuture<'a> {
        Box::pin(async move {
            let response = self
                .client
                .post(url)
                .header("content-type", "application/json")
                .bearer_auth(auth_token)
                .body(body)
                .send()
                .await
                .map_err(|error| StoreError::Database(error.without_uri().to_string()))?;
            let status = response.status();
            if !status.is_success() {
                return Err(StoreError::Database(format!(
                    "libSQL HTTP status {}",
                    status.as_u16()
                )));
            }
            let body = response
                .bytes()
                .await
                .map_err(|error| StoreError::Database(error.without_uri().to_string()))?;
            Ok(body.to_vec())
        })
    }
}
