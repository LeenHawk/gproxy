use base64::Engine as _;
use gproxy_admin::AdminError;
use gproxy_admin::dto::ExportSourceKeyDto;
use gproxy_store::records::CredentialEnvelope;

pub(super) fn open_credential(
    envelope: &CredentialEnvelope,
    source: &ExportSourceKeyDto,
    encoded: Option<&str>,
) -> Result<serde_json::Value, AdminError> {
    cipher(source, encoded)?.open(envelope).map_err(|_| {
        AdminError::BadRequest("credential secret could not be opened with the source key".into())
    })
}

pub(super) fn reseal_user_key(
    destination: &crate::secrets::EnvelopeCipher,
    envelope: &CredentialEnvelope,
    source: &ExportSourceKeyDto,
    encoded: Option<&str>,
) -> Result<CredentialEnvelope, AdminError> {
    let value = cipher(source, encoded)?
        .open_user_key(envelope)
        .map_err(|_| {
            AdminError::BadRequest("user key could not be opened with the source key".into())
        })?;
    destination
        .seal_user_key(&value)
        .map_err(|error| AdminError::Internal(error.to_string()))
}

fn cipher(
    source: &ExportSourceKeyDto,
    encoded: Option<&str>,
) -> Result<crate::secrets::EnvelopeCipher, AdminError> {
    let key = match source {
        ExportSourceKeyDto::Plaintext => None,
        ExportSourceKeyDto::Sealed { fingerprint } => {
            let encoded = encoded.ok_or_else(|| {
                AdminError::BadRequest("source_master_key is required for this export".into())
            })?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| {
                    AdminError::BadRequest("source_master_key must be standard base64".into())
                })?;
            let key: [u8; 32] = decoded.try_into().map_err(|_| {
                AdminError::BadRequest("source_master_key must decode to 32 bytes".into())
            })?;
            if crate::key_rotation::fingerprint(Some(&key)).as_deref() != Some(fingerprint) {
                return Err(AdminError::BadRequest(
                    "source_master_key does not match this export".into(),
                ));
            }
            Some(key)
        }
    };
    Ok(crate::secrets::EnvelopeCipher::new(key))
}
