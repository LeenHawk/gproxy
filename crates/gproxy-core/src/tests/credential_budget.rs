use super::*;

#[test]
fn exhausted_credential_is_skipped_before_egress_and_all_exhausted_returns_quota_error() {
    for all_exhausted in [false, true] {
        let host = MemoryHost::new(false);
        {
            let mut state = host.state.lock().unwrap();
            let mut second = target();
            second.credential = CredentialId(8);
            state.plan = Some(Plan {
                targets: vec![target(), second],
                budget: FailoverBudget { max_attempts: 1 },
            });
            state.exhausted_credentials = if all_exhausted {
                vec![CredentialId(7), CredentialId(8)]
            } else {
                vec![CredentialId(7)]
            };
            state.credential.secret = json!({"access_token": "fresh", "expires_at": i64::MAX});
        }
        let core = core(&host).unwrap();
        let result = block_on(core.execute(&host, request(false, "credential-budget")));
        let state = host.state.lock().unwrap();
        if all_exhausted {
            assert!(matches!(result, Err(CoreError::QuotaExceeded)));
            assert!(state.upstream_requests.is_empty());
            assert!(state.settlements.is_empty());
        } else {
            assert_eq!(result.unwrap().status, StatusCode::OK);
            assert_eq!(state.loaded_credentials, [CredentialId(8)]);
            assert_eq!(state.upstream_requests.len(), 1);
            assert_eq!(state.settlements[0].credential_id, CredentialId(8));
        }
    }
}
