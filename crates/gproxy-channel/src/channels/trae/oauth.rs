//! Desktop oauth/marscode/{util,request,login}.js: PKCE auth codes and a
//! persistent P-256 device key. DeviceProof signs six newline-separated fields
//! with SHA-256/DER ECDSA. The public authorization page redirects authCodeInfo
//! to auth_callback_url. A copied callback works with a remotely hosted gateway.
use super::{
    common::{check, insert, invalid, json, now_ms, random_hex},
    config::{CLIENT_ID, TraeConfig, fact},
};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, CredentialView, LoginContext, OAuthAuthorizationCode, OAuthCredential,
    OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::{require_success, send};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{HeaderMap, Method};
use p256::{
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::{EncodePublicKey, LineEnding},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) struct OAuth;

const TOKEN_PATH: &str = "/trae/api/v3/oauth/ExchangeToken";
const CALLBACK: &str = "http://127.0.0.1:1457/authorize";

fn private_key(value: &Value) -> Result<SigningKey, ChannelError> {
    let bytes = STANDARD
        .decode(value.as_str().ok_or(ChannelError::InvalidCredential)?)
        .map_err(|_| ChannelError::InvalidCredential)?;
    SigningKey::from_slice(&bytes).map_err(|_| ChannelError::InvalidCredential)
}

fn device(
    config: &TraeConfig,
    id: &str,
    machine: &str,
    key: &SigningKey,
) -> Result<Value, ChannelError> {
    let public = key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(invalid)?;
    Ok(
        json!({"DeviceID":id,"MachineID":machine,"PlatformCode":"IDE_PC","DeviceType":"PC",
        "DeviceName":"GProxy","DeviceModel":"","ClientVersion":config.version(),
        "DevicePublicKey":public,"DeviceBrand":"","DeviceCPU":"","OSInfo":"Linux","OSVersion":""}),
    )
}

fn headers(token: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    insert(&mut headers, "content-type", "application/json")?;
    insert(&mut headers, "x-cloudide-token", token)?;
    Ok(headers)
}

fn expiry(result: &Value) -> Option<i64> {
    let absolute = result["TokenExpireAt"].as_i64();
    let now = now_ms();
    match absolute {
        Some(at) if at > now => Some(at),
        _ => result["TokenExpireDuration"]
            .as_i64()
            .filter(|d| *d > 0)
            .map(|duration| now.saturating_add(duration))
            .or(absolute),
    }
}

async fn exchange(
    client: &dyn crate::OutboundClient,
    origin: &str,
    body: Value,
    token: &str,
    refreshing: bool,
) -> Result<Value, ChannelError> {
    let (status, _, bytes) = send(
        client,
        Method::POST,
        &format!("{origin}{TOKEN_PATH}"),
        headers(token)?,
        Some(body.to_string().into_bytes()),
    )
    .await?;
    // Only the native client's explicit invalid/revoked token codes are final.
    // HTTP 403 alone may be a device/risk policy refusal, not revocation.
    if refreshing
        && !status.is_server_error()
        && let Ok(value) = json(&bytes)
    {
        let code = value.pointer("/ResponseMetadata/Error/Code").map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        });
        if matches!(code.as_deref(), Some("20324" | "20101" | "20315" | "20126")) {
            return Err(ChannelError::RefreshRejected(
                "Trae rejected the refresh token".into(),
            ));
        }
    }
    let value = json(&require_success(status, bytes)?)?;
    check(&value)?;
    value
        .get("Result")
        .filter(|r| {
            r.get("Token")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        })
        .cloned()
        .ok_or_else(|| invalid("token exchange returned no Token"))
}

fn callback_result(code: &str, state: &str) -> Result<(String, Option<String>), ChannelError> {
    if let Ok(url) = url::Url::parse(code.trim()) {
        let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        if query.get("state").map(String::as_str) != Some(state) {
            return Err(ChannelError::InvalidCredential);
        }
        let info = query
            .get("authCodeInfo")
            .ok_or(ChannelError::InvalidCredential)?;
        let value: Value =
            serde_json::from_str(info).map_err(|_| ChannelError::InvalidCredential)?;
        let auth_code = value["AuthCode"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
        // Region is a closed selection, never an arbitrary callback host.
        let region = match query.get("host").map(String::as_str) {
            Some("https://grow-normal.traeapi.us" | "https://ug-normal.us.trae.ai") => {
                Some("USTTP".into())
            }
            _ => query
                .get("userRegion")
                .map(|s| super::config::region(s).to_owned()),
        };
        return Ok((auth_code.into(), region));
    }
    let code = code.trim();
    if code.is_empty() {
        return Err(ChannelError::InvalidCredential);
    }
    Ok((code.into(), None))
}

impl OAuthAuthorizationCode for OAuth {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = TraeConfig::from_view(context.provider)?;
            let mut bytes = [0; 32];
            getrandom::fill(&mut bytes)
                .map_err(|_| ChannelError::InvalidConfig("Trae: randomness unavailable".into()))?;
            let key = SigningKey::from_slice(&bytes).map_err(invalid)?;
            let id = random_hex::<16>()?;
            let machine = random_hex::<32>()?;
            let mut callback = url::Url::parse(CALLBACK).map_err(invalid)?;
            callback
                .query_pairs_mut()
                .append_pair("state", request.state);
            let mut url =
                url::Url::parse(&format!("{}/authorization", config.website())).map_err(invalid)?;
            url.query_pairs_mut().extend_pairs([
                ("state", request.state),
                ("login_version", "1"),
                ("auth_from", "trae"),
                ("login_channel", "native_ide"),
                ("plugin_version", config.build_version()),
                ("auth_type", "local"),
                ("client_id", CLIENT_ID),
                ("redirect", "0"),
                ("auth_callback_url", callback.as_str()),
                ("login_trace_id", request.state),
                ("machine_id", machine.as_str()),
                ("device_id", id.as_str()),
                ("x_device_id", id.as_str()),
                ("x_machine_id", machine.as_str()),
                ("x_app_version", config.version()),
                ("x_app_type", "stable"),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
            ]);
            Ok(AuthorizationStart {
                authorize_url: url.into(),
                // The host hands the whole pasted result to this channel.
                redirect_uri: String::new(),
                provider_state: BTreeMap::from([
                    ("device_key".into(), json!(STANDARD.encode(key.to_bytes()))),
                    ("device_id".into(), json!(id)),
                    ("machine_id".into(), json!(machine)),
                ]),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = TraeConfig::from_view(context.provider)?;
            let key_value = grant
                .provider_state
                .get("device_key")
                .ok_or(ChannelError::InvalidCredential)?;
            let key = private_key(key_value)?;
            let field = |name| {
                grant
                    .provider_state
                    .get(name)
                    .and_then(Value::as_str)
                    .ok_or(ChannelError::InvalidCredential)
            };
            let id = field("device_id")?;
            let machine = field("machine_id")?;
            let (code, region) = callback_result(grant.code, grant.state)?;
            let origin = config.auth_origin(region.as_deref());
            let result = exchange(
                context.client,
                origin,
                json!({"ClientID":CLIENT_ID,"AuthCode":code,
                "CodeVerifier":grant.code_verifier,"DeviceInfo":device(&config,id,machine,&key)?,
                "IDEVersion":config.version()}),
                "",
                false,
            )
            .await?;
            let token = result["Token"]
                .as_str()
                .ok_or(ChannelError::InvalidCredential)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{origin}/cloudide/api/v3/trae/GetUserInfo"),
                headers(token)?,
                Some(
                    json!({"ReqSource":"IDE","IDEVersion":config.version()})
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            let account = json(&require_success(status, body)?)?;
            check(&account)?;
            let account = account
                .get("Result")
                .ok_or_else(|| invalid("missing user information"))?;
            let user_id = account["UserID"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| invalid("missing UserID"))?;
            let mut fields = BTreeMap::from([
                ("device_id".into(), json!(id)),
                ("machine_id".into(), json!(machine)),
                ("user_id".into(), json!(user_id)),
                ("auth_region".into(), json!(region)),
            ]);
            for (name, value) in [
                (
                    "region",
                    region
                        .as_deref()
                        .filter(|r| *r == "USTTP")
                        .or_else(|| account["AIRegion"].as_str().map(super::config::region))
                        .or(region.as_deref()),
                ),
                ("user_name", account["ScreenName"].as_str()),
            ] {
                if let Some(value) = value {
                    fields.insert(name.into(), json!(value));
                }
            }
            Ok(OAuthCredential {
                access_token: token.into(),
                refresh_token: result["RefreshToken"].as_str().map(str::to_owned),
                id_token: None,
                token_type: Some("Cloud-IDE-JWT".into()),
                scopes: vec![],
                expires_at_ms: expiry(&result),
                refresh_expires_at_ms: result["RefreshExpireAt"].as_i64(),
                provider_fields: fields,
                provider_secrets: BTreeMap::from([("device_key".into(), key_value.clone())]),
            })
        })
    }
}

impl CredentialRefresh for OAuth {
    fn supports(&self, credential: &CredentialView<'_>) -> bool {
        credential.secret["refresh_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
            && credential
                .secret
                .pointer("/provider_secrets/device_key")
                .is_some()
    }
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = TraeConfig::from_view(context.provider)?;
            let credential = &context.credential;
            let key = private_key(
                credential
                    .secret
                    .pointer("/provider_secrets/device_key")
                    .ok_or(ChannelError::InvalidCredential)?,
            )?;
            let refresh = credential.secret["refresh_token"]
                .as_str()
                .ok_or(ChannelError::InvalidCredential)?;
            let token = credential.secret["access_token"]
                .as_str()
                .ok_or(ChannelError::InvalidCredential)?;
            let timestamp = now_ms() / 1000;
            let nonce = random_hex::<16>()?;
            let signature: Signature = key.sign(
                format!("POST\n{TOKEN_PATH}\n{CLIENT_ID}\n{refresh}\n{timestamp}\n{nonce}")
                    .as_bytes(),
            );
            let body = json!({"ClientID":CLIENT_ID,"ClientSecret":"","RefreshToken":refresh,
                "DeviceInfo":device(&config, fact(credential,"device_id").ok_or(ChannelError::InvalidCredential)?, fact(credential,"machine_id").ok_or(ChannelError::InvalidCredential)?, &key)?,
                "DeviceProof":{"Signature":STANDARD.encode(signature.to_der()),"Timestamp":timestamp,"Nonce":nonce},
                "IDEVersion":config.version()});
            let result = exchange(
                context.client,
                config.auth_origin(
                    fact(credential, "auth_region").or_else(|| fact(credential, "region")),
                ),
                body,
                token,
                true,
            )
            .await?;
            let expires_at_ms = expiry(&result);
            let mut secret = credential.secret.clone();
            secret["access_token"] = result["Token"].clone();
            if let Some(token) = result["RefreshToken"].as_str().filter(|s| !s.is_empty()) {
                secret["refresh_token"] = json!(token);
            }
            secret["expires_at_ms"] = json!(expires_at_ms);
            if result["RefreshExpireAt"].is_i64() {
                secret["refresh_expires_at_ms"] = result["RefreshExpireAt"].clone();
            }
            Ok(CredentialUpdate {
                secret,
                expires_at_ms,
            })
        })
    }
}
