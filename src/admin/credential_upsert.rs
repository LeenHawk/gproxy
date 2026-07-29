use serde_json::Value;

use crate::api::credentials::CredentialUpsert;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{Credential, CredentialInput};

pub(crate) enum CredentialUpsertPlan {
    Existing(Credential),
    Upsert(CredentialInput),
}

pub(crate) async fn plan_credential_upsert(
    state: &AppState,
    provider_id: i64,
    body: CredentialUpsert,
) -> Result<CredentialUpsertPlan, ApiError> {
    if body.id.is_none()
        && let Some(plain) = &body.secret_json
        && let Some(existing) = find_existing_token(state, provider_id, &body.kind, plain).await?
    {
        return Ok(CredentialUpsertPlan::Existing(existing));
    }

    // Auto-name on create when the caller supplied no label; updates keep the
    // caller's label verbatim (None clears, matching plain upsert semantics).
    let name = match (&body.label, body.id, &body.secret_json) {
        (None, None, Some(plain)) => crate::credentials::label::auto_label(&body.kind, plain),
        _ => body.label.clone(),
    };

    let secret_json = match (&body.secret_json, body.id) {
        (Some(plain), _) => state.cipher.seal(plain).map_err(internal)?,
        (None, Some(id)) => {
            let existing = state
                .persistence
                .get_credential(id)
                .await
                .map_err(internal)?
                .filter(|c| c.provider_id == provider_id)
                .ok_or_else(|| ApiError::NotFound("not found".into()))?;
            existing.secret_json
        }
        (None, None) => {
            return Err(ApiError::BadRequest(
                "secret_json required on create".into(),
            ));
        }
    };

    Ok(CredentialUpsertPlan::Upsert(CredentialInput {
        id: body.id,
        provider_id,
        name,
        kind: body.kind,
        secret_json,
        weight: body.weight,
        rpm_limit: body.rpm_limit,
        tpm_limit: body.tpm_limit,
        proxy_url: body.proxy_url,
        tls_fingerprint: body.tls_fingerprint,
        enabled: body.enabled,
    }))
}

/// Field carrying the bare token for single-token credential kinds. JSON
/// families (oauth/service-account) have no stable dedupe key and return None.
fn token_field(kind: &str) -> Option<&'static str> {
    match kind {
        "api_key" => Some("api_key"),
        "github_token" => Some("github_token"),
        _ => None,
    }
}

async fn find_existing_token(
    state: &AppState,
    provider_id: i64,
    kind: &str,
    plain: &Value,
) -> Result<Option<Credential>, ApiError> {
    let Some(field) = token_field(kind) else {
        return Ok(None);
    };
    let Some(token) = plaintext_token(field, plain) else {
        return Ok(None);
    };

    let credentials = state
        .persistence
        .list_credentials(provider_id)
        .await
        .map_err(internal)?;
    for credential in credentials {
        if credential.kind != kind {
            continue;
        }
        let opened = match state.cipher.open(&credential.secret_json) {
            Ok(opened) => opened,
            Err(e) => {
                tracing::warn!(
                    credential_id = credential.id,
                    error = %e,
                    "credential secret open failed during token duplicate check"
                );
                continue;
            }
        };
        if plaintext_token(field, &opened) == Some(token) {
            return Ok(Some(credential));
        }
    }

    Ok(None)
}

fn plaintext_token<'a>(field: &str, secret: &'a Value) -> Option<&'a str> {
    match secret {
        Value::String(s) => non_empty(s),
        Value::Object(obj) => obj.get(field).and_then(Value::as_str).and_then(non_empty),
        _ => None,
    }
}

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::Internal(e.to_string())
}
