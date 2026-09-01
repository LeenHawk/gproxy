use crate::records::{AuditEventInput, UserSessionInput};
use crate::{Store, StoreError};

#[derive(Debug, PartialEq)]
pub(super) struct Outcome {
    admin_id: i64,
    audit_events: usize,
    health: crate::records::CredentialHealthState,
}

pub(super) async fn run(store: &Store, user_key: i64) -> Result<Outcome, StoreError> {
    let admin_id = store
        .create_first_admin("admin", "argon2-hash")
        .await?
        .expect("first admin");
    assert_eq!(
        store.create_first_admin("second", "argon2-hash").await?,
        None
    );
    let token_digest = vec![9; 32];
    store
        .create_user_session(&UserSessionInput {
            token_digest: token_digest.clone(),
            user_id: admin_id,
            created_at: 100,
            expires_at: 200,
        })
        .await?;
    assert_eq!(
        store
            .admin_for_session(&token_digest, 150)
            .await?
            .expect("admin session")
            .id,
        admin_id
    );
    assert!(store.admin_for_session(&token_digest, 200).await?.is_none());
    store
        .record_audit_event(&AuditEventInput {
            actor_user_id: admin_id,
            action: "user_key.reveal".into(),
            target_kind: "user_key".into(),
            target_id: Some(user_key),
            at: 150,
            client_ip: Some("203.0.113.4".into()),
            details: None,
        })
        .await?;
    let audit = store.audit_events(10).await?;
    assert_eq!(audit[0].event.client_ip.as_deref(), Some("203.0.113.4"));
    let audit_events = audit.len();
    store
        .record_credential_health(&crate::records::CredentialHealthInput {
            credential_id: 1,
            model: "model-a".into(),
            credential_version: 0,
            version: 2,
            state: crate::records::CredentialHealthState::Dead,
            observed_at: 150,
            response_status: Some(401),
            detail: Some("rejected".into()),
        })
        .await?;
    store
        .record_credential_health(&crate::records::CredentialHealthInput {
            credential_id: 1,
            model: "model-b".into(),
            credential_version: 0,
            version: 1,
            state: crate::records::CredentialHealthState::Healthy,
            observed_at: 149,
            response_status: Some(200),
            detail: None,
        })
        .await?;
    let health_records = store.credential_health().await?;
    assert_eq!(health_records.len(), 2);
    assert_eq!(health_records[0].model, "model-a");
    assert_eq!(health_records[1].model, "model-b");
    let health = health_records[0].state;
    assert!(
        store
            .user_key_secret(user_key)
            .await?
            .expect("user key")
            .envelope
            .is_some()
    );
    Ok(Outcome {
        admin_id,
        audit_events,
        health,
    })
}
