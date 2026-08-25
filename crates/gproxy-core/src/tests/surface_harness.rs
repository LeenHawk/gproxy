use bytes::Bytes;
use http::Method;

use super::memory::MemoryHost;

pub(super) fn execute(
    core: &crate::Core<MemoryHost>,
    host: &MemoryHost,
    method: Method,
    path: &str,
    header: Option<(&str, &str)>,
    body: Option<&'static [u8]>,
) -> Result<serde_json::Value, crate::CoreError> {
    let outcome = outcome(core, host, method, path, header, body, false)?;
    let crate::ResponseBody::Full(body) = outcome.body else {
        panic!("test synth response was not buffered");
    };
    Ok(serde_json::from_slice(&body).expect("surface json"))
}

pub(super) fn outcome(
    core: &crate::Core<MemoryHost>,
    host: &MemoryHost,
    method: Method,
    path: &str,
    header: Option<(&str, &str)>,
    body: Option<&'static [u8]>,
    upgrade: bool,
) -> Result<crate::ExecOutcome, crate::CoreError> {
    let mut headers = http::HeaderMap::new();
    if let Some((name, value)) = header {
        headers.insert(
            http::header::HeaderName::try_from(name).expect("header name"),
            http::HeaderValue::try_from(value).expect("header value"),
        );
    }
    super::block_on(core.execute(
        host,
        crate::RequestCtx {
            request_id: format!("surface:{path}"),
            method,
            path: path.into(),
            query: None,
            headers,
            body: body.map_or_else(Bytes::new, Bytes::from_static),
            upgrade,
            mode: crate::RoutingMode::Aggregated,
        },
    ))
}

pub(super) fn plan(targets: Vec<crate::Target>) -> crate::Plan {
    crate::Plan {
        targets,
        budget: crate::control::FailoverBudget { max_attempts: 2 },
    }
}

pub(super) fn target(provider_id: i64, credential: i64) -> crate::Target {
    crate::Target {
        provider: crate::ProviderRef {
            id: provider_id,
            name: format!("provider-{provider_id}"),
            channel: "memory".into(),
            settings: serde_json::json!({"slot": credential}),
            fingerprint: None,
        },
        credential: crate::CredentialId(credential),
        upstream_model: "upstream-model".into(),
    }
}
