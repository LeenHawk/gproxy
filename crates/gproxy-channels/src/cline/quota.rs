use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaAvailability, QuotaBalance, QuotaEntry, QuotaKind, QuotaQueryMode,
    QuotaSource, QuotaSubject, QuotaSupport, QuotaValue,
};
use serde_json::Value;

pub(super) fn sources(secret: &Value) -> Vec<QuotaSource> {
    let ready = super::auth::field(secret, "access_token").is_some()
        && super::auth::field(secret, "user_id").is_some();
    vec![QuotaSource {
        id: "balance".into(), label: "Cline account credits".into(), kinds: vec![QuotaKind::Balance],
        mode: if ready { QuotaQueryMode::Probe } else { QuotaQueryMode::Unavailable },
        support: if ready { QuotaSupport::Ready } else { QuotaSupport::Unsupported },
        reason: Some(if ready {
            "Uses the official client's internal balance endpoint with the existing login token."
        } else {
            "Balance lookup needs the existing Cline login token and user identity. Manually supplied API key support is not confirmed; sign in through Cline to obtain that identity."
        }.into()),
        automatic: ready,
    }]
}

pub(super) fn prepare(
    source: &str,
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    if source != "balance" || sources(secret)[0].support != QuotaSupport::Ready {
        return Ok(None);
    }
    let user = super::auth::field(secret, "user_id").expect("validated by quota source");
    let uri = crate::shared::http::join(
        super::prepare::base_url(settings),
        &format!(
            "/users/{}/balance",
            crate::shared::http::encode_component(user)
        ),
        None,
    )?;
    let mut request = http::Request::get(crate::shared::http::strip_userinfo(uri)?)
        .body(Bytes::new())
        .map_err(|_| ChannelError::Prepare("Invalid Cline quota request".into()))?;
    super::auth::apply(request.headers_mut(), secret)?;
    Ok(Some(request))
}

pub(super) fn parse(
    source: &str,
    status: http::StatusCode,
    body: &[u8],
) -> Result<Vec<QuotaEntry>, ChannelError> {
    if source != "balance" || !status.is_success() {
        return Err(ChannelError::Prepare(format!(
            "Cline quota query failed (HTTP {status})"
        )));
    }
    let raw: Value = serde_json::from_slice(body)
        .map_err(|_| ChannelError::Prepare("Invalid Cline balance JSON".into()))?;
    if raw.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(ChannelError::Prepare(
            "Cline balance query did not report success".into(),
        ));
    }
    let data = &raw["data"];
    let remaining = crate::shared::quota_balances::amount(data, "balance")?;
    Ok(vec![crate::shared::quota_balances::entry(
        "balance",
        "balance:credits",
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(remaining),
            unit: Some("credits".into()),
            availability: QuotaAvailability::Unknown,
            components: vec![],
        }),
    )])
}
