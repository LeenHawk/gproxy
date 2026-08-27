use gproxy_core::error::StoreError;

use super::super::error;

pub(super) struct NativeSender {
    client: wreq::Client,
}

impl NativeSender {
    pub(super) fn new() -> Self {
        Self {
            client: wreq::Client::new(),
        }
    }

    pub(super) async fn post(
        &self,
        url: &str,
        token: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, StoreError> {
        let response = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .bearer_auth(token)
            .body(body)
            .send()
            .await
            .map_err(|_| error("Upstash", "request"))?;
        if !response.status().is_success() {
            return Err(error("Upstash", "request"));
        }
        response
            .bytes()
            .await
            .map(|value| value.to_vec())
            .map_err(|_| error("Upstash", "response"))
    }
}
