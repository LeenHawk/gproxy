//! M5 envelope integration: secrets sealed at import serve real traffic
//! (decrypt-at-use), and a master-key mismatch skips the credential without
//! ever reaching upstream.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use super::*;
use crate::crypto::{SecretCipher, cipher_from_master_key, envelope::is_envelope};
use crate::pipeline::error::PipelineError;

fn cipher(key_byte: u8) -> Arc<dyn SecretCipher> {
    cipher_from_master_key(Some(&B64.encode([key_byte; 32]))).expect("cipher")
}

fn msg_ok() -> Bytes {
    let body = json!({
        "id": "msg-1", "type": "message", "role": "assistant", "model": "claude-test",
        "content": [{ "type": "text", "text": "ok" }],
        "stop_reason": "end_turn", "stop_sequence": null,
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    });
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

#[tokio::test]
async fn sealed_secrets_serve_traffic() {
    let fake = Arc::new(FakeUpstream::new(msg_ok(), vec![]));
    let c = cipher(7);
    let (state, _dir) =
        state_with_ciphers(Arc::clone(&fake), BUNDLE, c.as_ref(), Arc::clone(&c)).await;

    // (a) the stored credential is an envelope, not the plaintext secret
    let creds = state.persistence.list_credentials(2).await.expect("list");
    assert_eq!(creds.len(), 1);
    assert!(
        is_envelope(&creds[0].secret_json),
        "stored secret_json must be sealed: {}",
        creds[0].secret_json
    );
    assert!(
        creds[0].secret_json["kek_id"]
            .as_str()
            .unwrap()
            .starts_with("local-")
    );

    // (b) claude passthrough succeeds end to end (decrypt-at-use)
    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-direct", false))
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    // (c) upstream received the REAL bearer key — open() recovered the
    // original secret, not envelope garbage (claudeapi auths via x-api-key)
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].headers.get("x-api-key").unwrap(), "up-key");
}

#[tokio::test]
async fn wrong_key_skips_credential() {
    let fake = Arc::new(FakeUpstream::new(msg_ok(), vec![]));
    // import seals under key A; the serving state opens with key B
    let (state, _dir) =
        state_with_ciphers(Arc::clone(&fake), BUNDLE, cipher(1).as_ref(), cipher(2)).await;

    let err = match crate::pipeline::execute(&state, claude_ctx("claude-direct", false)).await {
        Err(e) => e,
        Ok(_) => panic!("expected pipeline error"),
    };
    assert!(
        matches!(
            &err,
            PipelineError::Channel(crate::channel::ChannelError::InvalidCredential(_))
        ),
        "expected invalid-credential skip, got: {err}"
    );
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        0,
        "upstream must never see an attempt with an unopenable secret"
    );
    assert!(fake.seen.lock().unwrap().is_empty());
}
