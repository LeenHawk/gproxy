use base64::Engine as _;
use serde_json::json;

pub(super) const V2_SCHEMA: &str = r#"
CREATE TABLE providers(id INTEGER PRIMARY KEY,name TEXT,channel TEXT,label TEXT,settings_json TEXT,credential_strategy TEXT,proxy_url TEXT,tls_fingerprint TEXT,enabled INTEGER);
CREATE TABLE credentials(id INTEGER PRIMARY KEY,provider_id INTEGER,name TEXT,kind TEXT,secret_json TEXT,weight INTEGER,rpm_limit INTEGER,tpm_limit INTEGER,proxy_url TEXT,tls_fingerprint TEXT,enabled INTEGER);
CREATE TABLE routes(id INTEGER PRIMARY KEY,name TEXT,enabled INTEGER,strategy TEXT DEFAULT 'round_robin');
CREATE TABLE route_members(id INTEGER PRIMARY KEY,route_id INTEGER,provider_id INTEGER,upstream_model_id TEXT,tier INTEGER,weight INTEGER,enabled INTEGER);
CREATE TABLE aliases(id INTEGER PRIMARY KEY,provider TEXT,alias TEXT,target TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE price_rules(id INTEGER PRIMARY KEY,provider_id INTEGER,match_type TEXT,model_match TEXT,input_price TEXT,output_price TEXT,cache_read_price TEXT,cache_creation_5m_price TEXT,cache_creation_30m_price TEXT,cache_creation_1h_price TEXT,image_output_price TEXT,pricing_tiers_json TEXT,enabled INTEGER);
CREATE TABLE price_rule_rates(id INTEGER PRIMARY KEY,price_rule_id INTEGER,metric TEXT,unit_size INTEGER,price_usd TEXT,conditions_json TEXT,sort_order INTEGER);
CREATE TABLE orgs(id INTEGER PRIMARY KEY,name TEXT,enabled INTEGER);
CREATE TABLE teams(id INTEGER PRIMARY KEY,org_id INTEGER,name TEXT,enabled INTEGER);
CREATE TABLE users(id INTEGER PRIMARY KEY,name TEXT,org_id INTEGER,team_id INTEGER,password TEXT,enabled INTEGER,is_admin INTEGER);
CREATE TABLE user_keys(id INTEGER PRIMARY KEY,user_id INTEGER,api_key_ciphertext TEXT,api_key_digest TEXT,api_key_digest_version INTEGER,label TEXT,enabled INTEGER);
CREATE TABLE provider_models(id INTEGER PRIMARY KEY,provider_id INTEGER,model_id TEXT,display_name TEXT,variants_json TEXT,context_window INTEGER,max_output_tokens INTEGER,thinking_supported INTEGER,thinking_adaptive_supported INTEGER,thinking_enabled_supported INTEGER,enabled INTEGER);
CREATE TABLE quotas(id INTEGER PRIMARY KEY,scope TEXT,scope_id INTEGER,quota_total TEXT,quota_daily TEXT,quota_weekly TEXT,quota_monthly TEXT,quota_5h TEXT,quota_7d TEXT);
CREATE TABLE route_permissions(id INTEGER PRIMARY KEY,scope TEXT,scope_id INTEGER,route_pattern TEXT,created_at INTEGER,updated_at INTEGER);
CREATE TABLE routing_rules(id INTEGER PRIMARY KEY,provider_id INTEGER,operation TEXT,kind TEXT,implementation TEXT,dest_operation TEXT,dest_kind TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE rule_sets(id INTEGER PRIMARY KEY,name TEXT,description TEXT,enabled INTEGER);
CREATE TABLE rules(id INTEGER PRIMARY KEY,rule_set_id INTEGER,kind TEXT,config_json TEXT,filter_model_pattern TEXT,filter_operation_keys TEXT,filter_header_pattern TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE provider_rule_sets(id INTEGER PRIMARY KEY,provider_id INTEGER,rule_set_id INTEGER,sort_order INTEGER,enabled INTEGER);
CREATE TABLE instance_settings(id INTEGER PRIMARY KEY,instance_name TEXT,proxy TEXT,spoof_emulation INTEGER,enable_usage INTEGER,enable_upstream_log INTEGER,enable_upstream_log_body INTEGER,enable_downstream_log INTEGER,enable_downstream_log_body INTEGER,disable_log_redaction INTEGER,enable_tokenizer_download INTEGER,update_channel TEXT,enable_auto_update_check INTEGER,retention_days INTEGER,max_database_size_mb INTEGER,file_upload_max_in_flight INTEGER);
CREATE TABLE usages(id INTEGER PRIMARY KEY,request_id TEXT,at INTEGER,route_name TEXT,provider_id INTEGER,credential_id INTEGER,org_id INTEGER,team_id INTEGER,user_id INTEGER,user_key_id INTEGER,thread_id TEXT,operation TEXT,kind TEXT,model TEXT,input_tokens INTEGER,output_tokens INTEGER,image_output_tokens INTEGER,cache_read_tokens INTEGER,cache_creation_5m_tokens INTEGER,cache_creation_30m_tokens INTEGER,cache_creation_1h_tokens INTEGER,metrics_json TEXT,cost TEXT,latency_ms INTEGER,usage_source TEXT,ended TEXT);
"#;

pub(super) fn v2_database(
    directory: &std::path::Path,
    api_key: &str,
    stored_key: &str,
    credential: &serde_json::Value,
    with_usage: bool,
) {
    use tokio_rusqlite::rusqlite::{Connection, params};
    let connection = Connection::open(directory.join("gproxy.db")).unwrap();
    connection.execute_batch(V2_SCHEMA).unwrap();
    connection
        .execute("INSERT INTO orgs VALUES(1,'org',1)", [])
        .unwrap();
    connection
        .execute("INSERT INTO users VALUES(1,'user',1,NULL,NULL,1,0)", [])
        .unwrap();
    connection.execute("INSERT INTO providers VALUES(1,'provider','openai',NULL,'{}','round_robin',NULL,NULL,1)", []).unwrap();
    connection
        .execute(
            "INSERT INTO routes(id,name,enabled) VALUES(1,'public-model',1)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO route_members VALUES(1,1,1,'upstream-model',0,100,1)",
            [],
        )
        .unwrap();
    connection.execute("INSERT INTO provider_models VALUES(1,1,'upstream-model','Upstream model','[\"upstream-model-thinking-high\"]',128000,16384,1,1,1,1)", []).unwrap();
    connection.execute("INSERT INTO routing_rules VALUES(1,1,'generate_content','open_ai_chat_completions','transform_to','generate_content','open_ai_responses',0,1)", []).unwrap();
    connection.execute("INSERT INTO instance_settings(id,instance_name,proxy,spoof_emulation,enable_usage,enable_upstream_log,enable_upstream_log_body,enable_downstream_log,enable_downstream_log_body,disable_log_redaction,enable_tokenizer_download,update_channel,enable_auto_update_check,retention_days,max_database_size_mb,file_upload_max_in_flight) VALUES(1,'default',NULL,NULL,1,0,0,0,0,0,0,'staging',1,NULL,NULL,0)", []).unwrap();
    connection
        .execute(
            "INSERT INTO credentials VALUES(1,1,NULL,'api_key',?,100,NULL,NULL,NULL,NULL,1)",
            [credential.to_string()],
        )
        .unwrap();
    let digest = blake3::hash(api_key.strip_prefix("sk-").unwrap_or(api_key).as_bytes())
        .to_hex()
        .to_string();
    connection
        .execute(
            "INSERT INTO user_keys VALUES(1,1,?,?,2,NULL,1)",
            params![stored_key, digest],
        )
        .unwrap();
    if with_usage {
        connection.execute("INSERT INTO usages VALUES(1,'v2-request',1,NULL,1,1,1,NULL,1,1,NULL,'generate_content','openai_chat','model',2,3,0,1,0,0,0,'{}','12.34',4,'upstream','complete')", []).unwrap();
    }
}

pub(super) fn v2_seal(value: &serde_json::Value, key: [u8; 32]) -> serde_json::Value {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut dek = [0_u8; 32];
    let mut key_nonce = [0_u8; 24];
    let mut payload_nonce = [0_u8; 24];
    getrandom::fill(&mut dek).unwrap();
    getrandom::fill(&mut key_nonce).unwrap();
    getrandom::fill(&mut payload_nonce).unwrap();
    let kek_id = "local-test";
    let cipher = XChaCha20Poly1305::new(&Key::from(key));
    let mut wrapped = key_nonce.to_vec();
    wrapped.extend(
        cipher
            .encrypt(&XNonce::from(key_nonce), dek.as_slice())
            .unwrap(),
    );
    let ciphertext = XChaCha20Poly1305::new(&Key::from(dek))
        .encrypt(
            &XNonce::from(payload_nonce),
            Payload {
                msg: &serde_json::to_vec(value).unwrap(),
                aad: kek_id.as_bytes(),
            },
        )
        .unwrap();
    json!({
        "kek_id": kek_id,
        "wrapped_dek": base64::engine::general_purpose::STANDARD.encode(wrapped),
        "nonce": base64::engine::general_purpose::STANDARD.encode(payload_nonce),
        "ciphertext": base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}
