#![cfg(not(target_arch = "wasm32"))]

//! Deterministic ownership transfers between admission/read and the durable
//! operation. The peer deliberately never receives snapshot invalidation.
mod support;

use gproxy_sdk::{
    ClientPool, CredentialStatus, Gproxy, GproxyBuilder, RefreshMode, SyncMode,
    dto::{BatchItem, BatchPatch, CredentialPatch, CredentialWrite, ListQuery, ProviderWrite},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement};
use gproxy_store::{
    Store,
    entity::{identity::organization, upstream::credential},
};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Set, Statement,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

type TransferTrigger = Arc<Mutex<Option<(&'static str, usize)>>>;

struct TransferConnection {
    db: DatabaseConnection,
    trigger: TransferTrigger,
    secret: Vec<u8>,
}

#[async_trait::async_trait]
impl ConnectionTrait for TransferConnection {
    fn get_database_backend(&self) -> DbBackend {
        self.db.get_database_backend()
    }
    async fn execute_raw(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        self.db.execute_raw(stmt).await
    }
    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.db.execute_unprepared(sql).await
    }
    async fn query_one_raw(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.db.query_one_raw(stmt).await
    }
    async fn query_all_raw(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.db.query_all_raw(stmt).await
    }
}

#[async_trait::async_trait]
impl BatchConnectionTrait for TransferConnection {
    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        let transfer = {
            let mut armed = self.trigger.lock().unwrap();
            match armed.as_mut() {
                Some((needle, skip))
                    if statements
                        .iter()
                        .any(|s| s.statement().sql.contains(*needle)) =>
                {
                    if *skip == 0 {
                        armed.take();
                        true
                    } else {
                        *skip -= 1;
                        false
                    }
                }
                _ => false,
            }
        };
        if transfer {
            Store::new(self.db.clone())
                .credentials()
                .update_many(vec![credential::ActiveModel {
                    id: Set("c1".into()),
                    organization_id: Set(Some("new".into())),
                    secret: Set(self.secret.clone()),
                    version: Set(10),
                    ..Default::default()
                }])
                .await
                .unwrap();
        }
        self.db.batch(statements).await
    }
}

async fn setup() -> (
    Gproxy<DatabaseConnection>,
    Gproxy<TransferConnection>,
    TransferTrigger,
) {
    let (primary, channel, client) = support::sdk_parts().await;
    for id in ["old", "new"] {
        primary
            .store()
            .organizations()
            .create_many(vec![organization::ActiveModel {
                id: Set(id.into()),
                name: Set(id.into()),
                created_at_ms: Set(0),
                oauth_client_allowlist: Set(None),
            }])
            .await
            .unwrap();
    }
    primary
        .manage()
        .providers()
        .create(ProviderWrite {
            id: Some("p".into()),
            name: "provider".into(),
            channel: "test".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    primary
        .manage()
        .credentials()
        .create(CredentialWrite {
            id: Some("c1".into()),
            provider_id: "p".into(),
            auth_kind: "api_key".into(),
            organization_id: Some("old".into()),
            secret: json!({"api_key": "old-secret"}),
            ..Default::default()
        })
        .await
        .unwrap();
    let trigger = Arc::new(Mutex::new(None));
    let peer = GproxyBuilder::connection(TransferConnection {
        db: primary.store().connection().clone(),
        trigger: trigger.clone(),
        secret: primary
            .core()
            .secret_codec()
            .seal("c1", &json!({"api_key": "new-secret"}))
            .unwrap(),
    })
    .plaintext_secrets()
    .without_default_channels()
    .channel(channel)
    .client_pool(ClientPool::with_client(client))
    .sync_schema(false)
    .sync_mode(SyncMode::Manual)
    .build_unsynced()
    .await
    .unwrap();
    (primary, peer, trigger)
}

fn old_owner() -> ListQuery {
    ListQuery {
        owner_kind: Some("org".into()),
        owner_id: Some("old".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_transfer_between_read_and_mutation_rolls_back_the_entire_write() {
    for operation in ["update", "delete", "health", "status", "batch"] {
        let (primary, peer, trigger) = setup().await;
        let before = primary
            .store()
            .settings()
            .get()
            .await
            .unwrap()
            .unwrap()
            .config_revision;
        let manage = peer.manage();
        let credentials = manage.credentials().with_owner_filter(&old_owner());
        *trigger.lock().unwrap() = Some(("UPDATE \"credentials\"", 0));
        let result = match operation {
            "update" => credentials
                .update(
                    "c1",
                    CredentialPatch {
                        label: Some(Some("attacker".into())),
                        ..Default::default()
                    },
                )
                .await
                .map(|_| ()),
            "delete" => credentials.delete("c1").await,
            "health" => credentials.health_reset("c1").await.map(|_| ()),
            "status" => credentials
                .set_status("c1", CredentialStatus::Dead, None)
                .await
                .map(|_| ()),
            "batch" => credentials
                .batch(vec![
                    BatchItem::Create(CredentialWrite {
                        id: Some("must-rollback".into()),
                        provider_id: "p".into(),
                        auth_kind: "api_key".into(),
                        organization_id: Some("old".into()),
                        secret: json!({"api_key": "other"}),
                        ..Default::default()
                    }),
                    BatchItem::Update(BatchPatch {
                        id: "c1".into(),
                        patch: CredentialPatch {
                            enabled: Some(false),
                            ..Default::default()
                        },
                    }),
                ])
                .await
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert_eq!(result.unwrap_err().status_code(), 409, "{operation}");
        assert!(
            trigger.lock().unwrap().is_none(),
            "transfer did not run: {operation}"
        );
        let row = primary.manage().credentials().get("c1").await.unwrap();
        assert_eq!(row.organization_id.as_deref(), Some("new"));
        assert_ne!(row.label.as_deref(), Some("attacker"));
        assert!(row.enabled);
        assert_eq!(row.status, "active");
        assert_eq!(
            primary
                .manage()
                .credentials()
                .reveal_secret("c1")
                .await
                .unwrap(),
            json!({"api_key": "new-secret"})
        );
        assert!(
            primary
                .manage()
                .credentials()
                .get("must-rollback")
                .await
                .is_err()
        );
        assert_eq!(
            primary
                .store()
                .settings()
                .get()
                .await
                .unwrap()
                .unwrap()
                .config_revision,
            before
        );
    }
}

#[tokio::test]
async fn stale_peer_cannot_reveal_current_material_or_start_account_operations() {
    for operation in ["reveal", "refresh", "reset", "probe"] {
        let (_, peer, trigger) = setup().await;
        let manage = peer.manage();
        let credentials = manage.credentials().with_owner_filter(&old_owner());
        // Admission happened under the old owner. For account operations,
        // let the SDK's first read succeed and transfer before core reads the
        // material used by the channel.
        credentials.get("c1").await.unwrap();
        *trigger.lock().unwrap() =
            Some(("FROM \"credentials\"", usize::from(operation != "reveal")));
        let result = match operation {
            "reveal" => credentials.reveal_secret("c1").await.map(|_| ()),
            "refresh" => credentials
                .refresh("c1", RefreshMode::Force)
                .await
                .map(|_| ()),
            "reset" => credentials.quota_reset("c1").await.map(|_| ()),
            "probe" => credentials.quota_probe("c1").await.map(|_| ()),
            _ => unreachable!(),
        };
        assert_eq!(result.unwrap_err().status_code(), 404, "{operation}");
        assert!(
            trigger.lock().unwrap().is_none(),
            "transfer did not run: {operation}"
        );
    }
}

#[tokio::test]
async fn a_scoped_write_cannot_move_the_result_outside_its_scope() {
    let (primary, peer, _) = setup().await;
    let manage = peer.manage();
    let credentials = manage.credentials().with_owner_filter(&old_owner());
    let before = primary
        .store()
        .settings()
        .get()
        .await
        .unwrap()
        .unwrap()
        .config_revision;
    let error = credentials
        .update(
            "c1",
            CredentialPatch {
                organization_id: Some(Some("new".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 409);
    assert_eq!(
        primary
            .manage()
            .credentials()
            .get("c1")
            .await
            .unwrap()
            .organization_id
            .as_deref(),
        Some("old")
    );
    assert_eq!(
        primary
            .store()
            .settings()
            .get()
            .await
            .unwrap()
            .unwrap()
            .config_revision,
        before
    );
}
