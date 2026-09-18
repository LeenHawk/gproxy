use gproxy_core::{AesGcmCodec, PlaintextCodec, SecretCodec, SecretError};
use serde_json::json;

#[test]
fn aes_envelope_round_trips_and_is_bound_to_key_and_credential() {
    let codec = AesGcmCodec::new([7; 32]);
    let secret = json!({"access_token": "a", "refresh_token": "r"});
    let sealed = codec.seal("c1", &secret).unwrap();
    assert_eq!(sealed[0], 0x01);
    assert!(!sealed.windows(2).any(|w| w == b"\"a"));
    assert_eq!(codec.open("c1", &sealed).unwrap(), secret);
    assert_eq!(codec.open("c2", &sealed), Err(SecretError::Open));
    assert_eq!(
        AesGcmCodec::new([8; 32]).open("c1", &sealed),
        Err(SecretError::Open)
    );
    let mut tampered = sealed.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert_eq!(codec.open("c1", &tampered), Err(SecretError::Open));
    assert_ne!(codec.seal("c1", &secret).unwrap(), sealed);
}

#[test]
fn codecs_refuse_each_others_envelopes() {
    let secret = json!({"api_key": "k"});
    let plain = PlaintextCodec.seal("c1", &secret).unwrap();
    assert_eq!(plain[0], 0x00);
    assert_eq!(PlaintextCodec.open("c1", &plain).unwrap(), secret);
    let keyed = AesGcmCodec::new([1; 32]);
    assert_eq!(keyed.open("c1", &plain), Err(SecretError::Open));
    let sealed = keyed.seal("c1", &secret).unwrap();
    assert_eq!(PlaintextCodec.open("c1", &sealed), Err(SecretError::Open));
    assert_eq!(PlaintextCodec.open("c1", &[]), Err(SecretError::Open));
    assert_eq!(keyed.open("c1", &[0x01, 0, 0]), Err(SecretError::Open));
}
