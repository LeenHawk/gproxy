use super::super::quota_balances::{amount, entry};
use super::{
    field, invalid,
    prepare::{endpoint, segment},
    setting,
};
use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaAllowance, QuotaEntry, QuotaSubject, QuotaValue};
use http::Request;
use serde_json::{Value, json};

pub(super) fn identity(secret: &Value) -> Option<Value> {
    let separate = field(secret, "quota_access_key_id").is_some()
        || field(secret, "quota_access_key_secret").is_some();
    let (access, key, token) = if separate {
        (
            field(secret, "quota_access_key_id")?,
            field(secret, "quota_access_key_secret")?,
            field(secret, "quota_session_token"),
        )
    } else {
        (
            field(secret, "access_key_id")?,
            field(secret, "secret_access_key")?,
            field(secret, "session_token"),
        )
    };
    Some(json!({"access_key_id":access, "secret_access_key":key, "session_token":token}))
}

pub(super) fn prepare(
    secret: &Value,
    settings: &Value,
    cursor: Option<&str>,
) -> Result<Request<Bytes>, ChannelError> {
    let identity = identity(secret)
        .ok_or_else(|| ChannelError::Secret("AWS management access key pair is required".into()))?;
    let region =
        segment(setting(secret, settings, "quota_region", "region").unwrap_or("us-east-1"))?;
    let (operation, body) = if let Some(code) = field(secret, "quota_quota_code") {
        if cursor.is_some() {
            return Err(invalid(
                "A single AWS quota query cannot continue a list cursor",
            ));
        }
        (
            "GetServiceQuota",
            json!({"ServiceCode":"bedrock", "QuotaCode":code}),
        )
    } else {
        let mut body = json!({"ServiceCode":"bedrock", "MaxResults":100});
        if let Some(cursor) = cursor {
            body["NextToken"] = cursor.into();
        }
        ("ListServiceQuotas", body)
    };
    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    let uri = endpoint(
        secret,
        &format!("https://servicequotas.{region}.{suffix}"),
        "/",
    )?;
    let mut request = Request::post(uri)
        .header("content-type", "application/x-amz-json-1.1")
        .header(
            "x-amz-target",
            format!("ServiceQuotasV20190624.{operation}"),
        )
        .body(Bytes::from(body.to_string()))
        .map_err(|_| invalid("Invalid AWS quota request"))?;
    crate::aws_bedrock::auth::apply_service(&mut request, &identity, region, "servicequotas")?;
    Ok(request)
}

pub(super) fn parse(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let quotas = if let Some(quota) = raw.get("Quota") {
        std::slice::from_ref(quota)
    } else {
        raw.get("Quotas")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Missing AWS service quotas"))?
    };
    quotas
        .iter()
        .map(|quota| {
            if quota
                .get("ErrorReason")
                .is_some_and(|value| !value.is_null())
            {
                return Err(invalid("AWS could not retrieve an applied service quota"));
            }
            let id = field(quota, "QuotaArn")
                .or_else(|| field(quota, "QuotaCode"))
                .ok_or_else(|| invalid("Missing AWS quota identifier"))?;
            let mut entry = entry(
                "management_quota",
                id,
                QuotaSubject::Account,
                QuotaValue::RateLimit(QuotaAllowance {
                    limit: Some(amount(quota, "Value")?),
                    unit: field(quota, "Unit").map(str::to_owned),
                    ..Default::default()
                }),
            );
            entry.label = field(quota, "QuotaName").map(str::to_owned);
            Ok(entry)
        })
        .collect()
}
