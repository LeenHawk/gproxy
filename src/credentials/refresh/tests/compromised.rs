use super::*;

struct DelayedInvalidGrant;

#[async_trait]
impl Channel for DelayedInvalidGrant {
    fn id(&self) -> &'static str {
        "delayed_invalid_grant"
    }

    fn provider_family(&self) -> crate::protocol::Provider {
        crate::protocol::Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        Vec::new()
    }

    fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Unsupported("prepare"))
    }

    fn classify(
        &self,
        status: StatusCode,
        headers: &http::HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        Disposition::from_http(status, headers)
    }

    fn needs_refresh(&self, _secret: &Value) -> bool {
        true
    }

    async fn refresh(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _secret: &Value,
    ) -> Result<Value, ChannelError> {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        Err(ChannelError::InvalidCredential("invalid_grant".into()))
    }
}

#[tokio::test]
async fn lost_lease_adopts_peer_after_invalid_grant() {
    let cipher = cipher();
    let (mut state, cred, _dir) =
        state_with_cred(Arc::clone(&cipher), json!({"access_token": "old"})).await;
    state.cache = Arc::new(super::distributed_lock::LostLeaseCache(MemoryCache::new()));
    let channel: Arc<dyn Channel> = Arc::new(DelayedInvalidGrant);
    let provider = test_provider();

    let (result, _) = tokio::join!(
        state.ensure_fresh_credential(
            &channel,
            &cred,
            &provider,
            json!({"access_token": "old"}),
            false,
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            PersistenceBackend::upsert_credential(
                state.persistence.as_ref(),
                CredentialInput {
                    id: Some(cred.id),
                    provider_id: cred.provider_id,
                    name: cred.name.clone(),
                    kind: cred.kind.clone(),
                    secret_json: cipher.seal(&json!({"access_token": "peer"})).unwrap(),
                    weight: cred.weight,
                    rpm_limit: cred.rpm_limit,
                    tpm_limit: cred.tpm_limit,
                    proxy_url: cred.proxy_url.clone(),
                    tls_fingerprint: cred.tls_fingerprint.clone(),
                    enabled: true,
                },
            )
            .await
            .unwrap();
        },
    );

    assert_eq!(result.unwrap(), json!({"access_token": "peer"}));
}
