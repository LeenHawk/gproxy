use std::sync::Arc;

use gproxy_core::channel_api::{CODEX_OAUTH_CLIENT_ID, PI_OAUTH_CLIENT_ID};

use super::super::{Executor, Statement, native::NativeSql};
use crate::records::*;
use crate::schema::{Dialect, SchemaVersion};

#[tokio::test]
async fn upgrading_preserves_legacy_tokens_and_does_not_reseed_deleted_clients() {
    for remote in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let executor = Arc::new(NativeSql::open(path.clone()).await.unwrap());
        crate::migration::migrate_to(
            executor.as_ref(),
            Dialect::NativeSqlite,
            SchemaVersion::ModelMetadata,
        )
        .await
        .unwrap();
        let old = super::store(executor.clone());
        let user = old
            .insert_user(&UserInput {
                name: "legacy-user".into(),
                organization_id: None,
                team_id: None,
                password_hash: None,
                enabled: true,
                is_admin: false,
            })
            .await
            .unwrap();
        let key = old
            .insert_user_key(&UserKeyInput {
                user_id: user,
                digest: vec![7; 32],
                digest_version: 1,
                prefix: "oauth".into(),
                envelope: CredentialEnvelope {
                    ciphertext: vec![],
                    wrapped_key: vec![],
                    payload_nonce: vec![],
                    key_nonce: vec![],
                },
                label: None,
                expires_at: None,
                enabled: true,
            })
            .await
            .unwrap();
        let grant = old
            .insert_oauth_grant(&OAuthGrantInput {
                user_id: user,
                user_key_id: key,
                provider_id: Some(7),
                client_id: CODEX_OAUTH_CLIENT_ID.into(),
                scopes: "openid profile offline_access".into(),
                chatgpt_user_id: "legacy-user".into(),
                chatgpt_account_id: "legacy-account".into(),
                created_at: 90,
            })
            .await
            .unwrap();
        for (digest, kind, created_at) in [
            (11, "access", 100),
            (12, "refresh", 100),
            (13, "access", 120),
            (14, "refresh", 120),
        ] {
            old.insert_oauth_token(&OAuthTokenInput {
                digest: vec![digest; 32],
                grant_id: grant,
                kind: kind.into(),
                created_at,
                expires_at: 2_000,
            })
            .await
            .unwrap();
        }
        executor
            .execute(Statement::plain(
                "UPDATE oauth_tokens SET consumed_at=120 WHERE kind='refresh' AND created_at=100",
            ))
            .await
            .unwrap();
        drop(old);
        drop(executor);
        let (store, _) = if remote {
            super::libsql_store(path).await.unwrap()
        } else {
            super::native_store(path).await.unwrap()
        };
        let identity = store
            .oauth_access_identity(&[13; 32], 130)
            .await
            .unwrap()
            .unwrap();
        assert_eq!((identity.user_id, identity.user_key_id), (user, key));
        let page = store.oauth_sessions(user, 130, false, 10, 0).await.unwrap();
        assert_eq!((page.total_logins, page.active_sessions), (1, 1));
        let session = &page.sessions[0];
        assert_eq!(
            (
                session.logged_in_at,
                session.refresh_count,
                session.last_refreshed_at
            ),
            (100, Some(1), Some(120))
        );
        let refresh = store.oauth_token(&[14; 32]).await.unwrap().unwrap();
        assert_eq!(refresh.grant.provider_id, Some(7));
        assert!(
            store
                .exchange_oauth_tokens(
                    OAuthExchangeSource::Refresh(refresh.id),
                    CODEX_OAUTH_CLIENT_ID,
                    &OAuthTokenInput {
                        digest: vec![15; 32],
                        grant_id: grant,
                        kind: "access".into(),
                        created_at: 140,
                        expires_at: 2_000
                    },
                    &OAuthTokenInput {
                        digest: vec![16; 32],
                        grant_id: grant,
                        kind: "refresh".into(),
                        created_at: 140,
                        expires_at: 3_000
                    },
                )
                .await
                .unwrap()
        );
        let client = store
            .oauth_client(PI_OAUTH_CLIENT_ID)
            .await
            .unwrap()
            .unwrap();
        store
            .update_oauth_client(
                client.id,
                &OAuthClientInput {
                    client_id: client.client_id,
                    name: client.name,
                    redirect_uris: client.redirect_uris,
                    enabled: false,
                },
                Some(150),
                150,
            )
            .await
            .unwrap();
        crate::migration::migrate(
            store.backend(),
            if remote {
                Dialect::Libsql
            } else {
                Dialect::NativeSqlite
            },
        )
        .await
        .unwrap();
        assert_eq!(
            store
                .oauth_client(PI_OAUTH_CLIENT_ID)
                .await
                .unwrap()
                .unwrap()
                .deleted_at,
            Some(150)
        );
        assert!(
            store
                .oauth_access_identity(&[15; 32], 160)
                .await
                .unwrap()
                .is_some()
        );
    }
}
