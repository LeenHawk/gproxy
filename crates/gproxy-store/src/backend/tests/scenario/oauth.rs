use crate::records::*;
use crate::{Store, StoreError};

pub(super) async fn run(store: &Store) -> Result<OAuthSessionPage, StoreError> {
    let client = OAuthClientInput {
        client_id: "parity-client".into(),
        name: "Parity application".into(),
        redirect_uris: vec!["http://127.0.0.1/oauth/callback".into()],
        enabled: true,
    };
    let client_id = store.insert_oauth_client(&client).await?;
    let user = store
        .insert_user(&UserInput {
            name: "oauth-parity".into(),
            organization_id: None,
            team_id: None,
            password_hash: None,
            enabled: true,
            is_admin: false,
        })
        .await?;
    let first = authorization(store, user, 31).await?;
    assert_eq!(
        store
            .oauth_sessions(user, 101, false, 10, 0)
            .await?
            .total_logins,
        0
    );
    let access = token(first.grant.id, 41, "access", 110);
    let refresh = token(first.grant.id, 42, "refresh", 110);
    assert!(
        store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Code(first.id),
                &client.client_id,
                &access,
                &refresh
            )
            .await?
    );
    assert!(
        !store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Code(first.id),
                &client.client_id,
                &token(first.grant.id, 43, "access", 111),
                &token(first.grant.id, 44, "refresh", 111)
            )
            .await?
    );
    let second = authorization(store, user, 32).await?;
    assert!(
        store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Code(second.id),
                &client.client_id,
                &token(second.grant.id, 45, "access", 112),
                &token(second.grant.id, 46, "refresh", 112)
            )
            .await?
    );
    let page = store.oauth_sessions(user, 115, true, 1, 0).await?;
    assert_eq!(
        (page.total_logins, page.active_sessions, page.sessions.len()),
        (2, 2, 1)
    );
    assert!(
        store
            .oauth_access_identity(&access.digest, 115)
            .await?
            .is_some()
    );
    let refresh_id = store.oauth_token(&refresh.digest).await?.unwrap().id;
    let next_access = token(first.grant.id, 47, "access", 120);
    let next_refresh = token(first.grant.id, 48, "refresh", 120);
    let other_access = token(first.grant.id, 49, "access", 120);
    let other_refresh = token(first.grant.id, 50, "refresh", 120);
    let source = OAuthExchangeSource::Refresh(refresh_id);
    let (left, right) = tokio::join!(
        store.exchange_oauth_tokens(source, &client.client_id, &next_access, &next_refresh),
        store.exchange_oauth_tokens(source, &client.client_id, &other_access, &other_refresh),
    );
    let left = left?;
    assert_ne!(left, right?);
    let current = if left { next_refresh } else { other_refresh };
    let page = store.oauth_sessions(user, 125, false, 10, 0).await?;
    assert_eq!(page.total_logins, 2);
    let session = page
        .sessions
        .iter()
        .find(|session| session.id == first.grant.id)
        .unwrap();
    assert_eq!(
        (session.refresh_count, session.last_refreshed_at),
        (Some(1), Some(120))
    );
    let current_id = store.oauth_token(&current.digest).await?.unwrap().id;
    assert!(
        store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Refresh(current_id),
                &client.client_id,
                &access,
                &token(first.grant.id, 51, "refresh", 130)
            )
            .await
            .is_err()
    );
    assert!(
        store
            .oauth_token(&current.digest)
            .await?
            .unwrap()
            .consumed_at
            .is_none()
    );
    assert!(
        !store
            .revoke_owned_oauth_session(user + 1, second.grant.id, 140)
            .await?
    );
    assert!(
        store
            .revoke_owned_oauth_session(user, second.grant.id, 140)
            .await?
    );
    assert!(
        store
            .revoke_owned_oauth_session(user, second.grant.id, 141)
            .await?
    );
    assert_eq!(
        store
            .oauth_sessions(user, 150, true, 10, 0)
            .await?
            .active_sessions,
        1
    );
    assert_eq!(
        store
            .oauth_sessions(user, 2_000, true, 10, 0)
            .await?
            .active_sessions,
        0
    );
    let pending = authorization(store, user, 33).await?;
    store
        .update_oauth_client(
            client_id,
            &OAuthClientInput {
                enabled: false,
                ..client.clone()
            },
            None,
            160,
        )
        .await?;
    store
        .update_oauth_client(client_id, &client, None, 161)
        .await?;
    assert!(
        store
            .oauth_access_identity(&access.digest, 162)
            .await?
            .is_none()
    );
    assert!(
        !store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Code(pending.id),
                &client.client_id,
                &token(pending.grant.id, 52, "access", 162),
                &token(pending.grant.id, 53, "refresh", 162)
            )
            .await?
    );
    assert!(
        !store
            .exchange_oauth_tokens(
                OAuthExchangeSource::Refresh(current_id),
                &client.client_id,
                &token(first.grant.id, 54, "access", 162),
                &token(first.grant.id, 55, "refresh", 162)
            )
            .await?
    );
    store
        .update_oauth_client(client_id, &client, Some(170), 170)
        .await?;
    let page = store.oauth_sessions(user, 180, false, 10, 0).await?;
    assert_eq!((page.total_logins, page.active_sessions), (2, 0));
    Ok(page)
}

async fn authorization(store: &Store, user: i64, value: u8) -> Result<OAuthCodeRecord, StoreError> {
    assert!(
        store
            .create_oauth_authorization(&OAuthAuthorizationInput {
                key: UserKeyInput {
                    user_id: user,
                    digest: vec![value; 32],
                    digest_version: 1,
                    prefix: "oauth".into(),
                    envelope: super::seed::envelope(value),
                    label: None,
                    expires_at: None,
                    enabled: true,
                },
                provider_id: None,
                client_id: "parity-client".into(),
                scopes: "gproxy".into(),
                chatgpt_user_id: String::new(),
                chatgpt_account_id: String::new(),
                code_digest: vec![value; 32],
                redirect_uri: "http://127.0.0.1/oauth/callback".into(),
                code_challenge: "challenge".into(),
                created_at: 100,
                expires_at: 200,
            })
            .await?
    );
    Ok(store.oauth_code(&[value; 32]).await?.unwrap())
}

fn token(grant_id: i64, value: u8, kind: &str, now: i64) -> OAuthTokenInput {
    OAuthTokenInput {
        digest: vec![value; 32],
        grant_id,
        kind: kind.into(),
        created_at: now,
        expires_at: now + 1_000,
    }
}
