use gproxy_channel_api::{Disposition, ResponseView};

pub(crate) fn classify(response: ResponseView<'_>) -> Disposition {
    if response.status == http::StatusCode::FORBIDDEN
        && serde_json::from_slice::<serde_json::Value>(response.body)
            .ok()
            .is_some_and(|value| {
                value
                    .pointer("/error/code")
                    .and_then(serde_json::Value::as_str)
                    == Some("misalignment_policy_violation")
            })
    {
        return Disposition::Terminal;
    }
    crate::shared::disposition::unauthorized_or_forbidden(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misalignment_block_is_terminal_without_killing_the_credential() {
        let headers = http::HeaderMap::new();
        let body =
            br#"{"error":{"type":"invalid_request_error","code":"misalignment_policy_violation"}}"#;
        assert_eq!(
            classify(ResponseView {
                status: http::StatusCode::FORBIDDEN,
                headers: &headers,
                body,
            }),
            Disposition::Terminal
        );
        assert_eq!(
            classify(ResponseView {
                status: http::StatusCode::FORBIDDEN,
                headers: &headers,
                body: b"{}",
            }),
            Disposition::CredentialDead
        );
    }
}
