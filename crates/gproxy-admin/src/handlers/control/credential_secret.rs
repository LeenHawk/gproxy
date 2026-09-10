use crate::{AdminError, State, dto::CredentialWriteRequest};
use gproxy_store::records::CredentialUpdateInput;
use serde_json::{Map, Value};

pub(super) async fn create(
    state: &impl State,
    request: &CredentialWriteRequest,
) -> Result<Value, AdminError> {
    let primary = request
        .secret
        .as_ref()
        .ok_or_else(|| AdminError::BadRequest("credential secret is required".into()))?;
    merge(state, request, primary, None, false).await
}

pub(super) async fn update(
    state: &impl State,
    id: i64,
    request: &CredentialWriteRequest,
) -> Result<(bool, bool), AdminError> {
    for _ in 0..3 {
        let current = state
            .store()
            .credential(id)
            .await?
            .ok_or(AdminError::NotFound)?;
        let previous = if request.secret.is_some()
            || request.quota_secret.is_some()
            || current.provider_id != request.provider_id
        {
            Some(state.reveal_credential_secret(id).await?)
        } else {
            None
        };
        let envelope = match previous.as_ref() {
            Some(previous) => Some(
                state.seal_credential(
                    &merge(
                        state,
                        request,
                        request.secret.as_ref().unwrap_or(previous),
                        Some(previous),
                        current.provider_id == request.provider_id,
                    )
                    .await?,
                )?,
            ),
            None => None,
        };
        let meta = state
            .store()
            .admin_credentials()
            .await?
            .into_iter()
            .find(|value| value.id == id)
            .ok_or(AdminError::NotFound)?;
        let tls_fingerprint = request
            .tls_fingerprint
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| AdminError::BadRequest(error.to_string()))?;
        let preserve_health = request.secret.is_none()
            && request.quota_secret.is_some()
            && current.provider_id == request.provider_id
            && current.kind == request.kind
            && current.enabled == request.enabled
            && meta.proxy_url == request.proxy_url
            && meta.tls_fingerprint == tls_fingerprint;
        let applied = state
            .store()
            .update_credential_version(
                id,
                &CredentialUpdateInput {
                    provider_id: request.provider_id,
                    label: request.label.clone(),
                    kind: request.kind.clone(),
                    envelope,
                    enabled: request.enabled,
                    weight: request.weight,
                    rpm_limit: request.rpm_limit,
                    tpm_limit: request.tpm_limit,
                    proxy_url: request.proxy_url.clone(),
                    tls_fingerprint,
                },
                current.version,
                preserve_health,
            )
            .await?;
        if applied {
            return Ok((true, !preserve_health));
        }
    }
    Err(AdminError::Conflict(
        "credential changed concurrently; retry saving".into(),
    ))
}

async fn merge(
    state: &impl State,
    request: &CredentialWriteRequest,
    primary: &Value,
    previous: Option<&Value>,
    keep_previous_quota: bool,
) -> Result<Value, AdminError> {
    let mut value = primary
        .as_object()
        .cloned()
        .ok_or_else(|| AdminError::BadRequest("credential secret must be an object".into()))?;
    if request.secret.is_some() && value.keys().any(|key| key.starts_with("quota_")) {
        return Err(AdminError::BadRequest(
            "use quota_secret to configure quota authorization".into(),
        ));
    }
    value.retain(|key, _| !key.starts_with("quota_"));
    let provider = state
        .store()
        .control_snapshot()
        .await?
        .providers
        .into_iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or(AdminError::NotFound)?;
    let channel = state
        .channel_catalogue()
        .into_iter()
        .find(|channel| channel.id == provider.channel)
        .ok_or(AdminError::NotFound)?;
    if keep_previous_quota
        && let Some(previous) = previous.and_then(Value::as_object)
        && previous
            .get("quota_channel")
            .and_then(Value::as_str)
            .is_none_or(|bound| bound == provider.channel)
    {
        value.retain(|key, _| !key.starts_with("quota_"));
        value.extend(
            previous
                .iter()
                .filter(|(key, _)| key.starts_with("quota_"))
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    if let Some(patch) = request.quota_secret.as_ref() {
        let patch = patch
            .as_object()
            .ok_or_else(|| AdminError::BadRequest("quota_secret must be an object".into()))?;
        for (key, field) in patch {
            if !key.starts_with("quota_")
                || !channel.quota_fields.iter().any(|field| field.key == *key)
            {
                return Err(AdminError::BadRequest(format!(
                    "unsupported quota authorization field: {key}"
                )));
            }
            apply(&mut value, key, field)?;
        }
    }
    if request.quota_secret.is_some()
        && value
            .keys()
            .any(|key| key.starts_with("quota_") && key != "quota_channel")
    {
        value.insert("quota_channel".into(), Value::String(provider.channel));
    } else if !value
        .keys()
        .any(|key| key.starts_with("quota_") && key != "quota_channel")
    {
        value.remove("quota_channel");
    }
    Ok(Value::Object(value))
}

fn apply(value: &mut Map<String, Value>, key: &str, input: &Value) -> Result<(), AdminError> {
    match input {
        Value::Null => {
            value.remove(key);
        }
        Value::String(text) if text.trim().is_empty() => {
            value.remove(key);
        }
        Value::String(text) => {
            value.insert(key.into(), Value::String(text.trim().into()));
        }
        _ => {
            return Err(AdminError::BadRequest(format!(
                "{key} must be a string or null"
            )));
        }
    }
    Ok(())
}
