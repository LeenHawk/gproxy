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
    OperationFuture, RefreshContext, RefreshReason,
};
use crate::channels::shared::compatible::ability::{require_success, send};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{HeaderMap, Method};
use p256::{
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::{EncodePublicKey, LineEnding},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) struct OAuth;

const TOKEN_PATH: &str = "/trae/api/v3/oauth/ExchangeToken";
const CALLBACK: &str = "http://127.0.0.1:1457/authorize";
/// authUtil.js X8: renew once a day or less remains (`b$`)...
const REFRESH_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
/// ...or a third of the lifetime since `tokenReleaseAt`. Kept with the sealed
/// login material so a refresh replaces it without touching public metadata.
const RELEASED_AT: &str = "token_release_at_ms";

fn private_key(value: &Value) -> Result<SigningKey, ChannelError> {
    let bytes = STANDARD
        .decode(value.as_str().ok_or(ChannelError::InvalidCredential)?)
        .map_err(|_| ChannelError::InvalidCredential)?;
    SigningKey::from_slice(&bytes).map_err(|_| ChannelError::InvalidCredential)
}

// Request bodies are structs so they serialize in the desktop's insertion
// order (marscode/request.js); serde_json maps would sort the keys.

/// marscode/request.js `j()`: the Linux desktop's system information.
#[derive(Serialize)]
struct DeviceInfo<'a> {
    #[serde(rename = "DeviceID")]
    device_id: &'a str,
    #[serde(rename = "MachineID")]
    machine_id: &'a str,
    #[serde(rename = "PlatformCode")]
    platform_code: &'static str,
    #[serde(rename = "DeviceType")]
    device_type: &'static str,
    #[serde(rename = "DeviceName")]
    device_name: String,
    #[serde(rename = "DeviceModel")]
    device_model: &'static str,
    #[serde(rename = "ClientVersion")]
    client_version: &'a str,
    #[serde(rename = "DevicePublicKey")]
    device_public_key: String,
    #[serde(rename = "DeviceBrand")]
    device_brand: &'static str,
    #[serde(rename = "DeviceCPU")]
    device_cpu: &'static str,
    #[serde(rename = "OSInfo")]
    os_info: &'static str,
    #[serde(rename = "OSVersion")]
    os_version: &'static str,
}

#[derive(Serialize)]
struct DeviceProof {
    #[serde(rename = "Signature")]
    signature: String,
    #[serde(rename = "Timestamp")]
    timestamp: i64,
    #[serde(rename = "Nonce")]
    nonce: String,
}

#[derive(Serialize)]
struct RefreshBody<'a> {
    #[serde(rename = "ClientID")]
    client_id: &'static str,
    #[serde(rename = "ClientSecret")]
    client_secret: &'static str,
    #[serde(rename = "RefreshToken")]
    refresh_token: &'a str,
    #[serde(rename = "DeviceInfo")]
    device_info: DeviceInfo<'a>,
    #[serde(rename = "DeviceProof")]
    device_proof: DeviceProof,
    #[serde(rename = "IDEVersion")]
    ide_version: &'a str,
}

#[derive(Serialize)]
struct AuthCodeBody<'a> {
    #[serde(rename = "ClientID")]
    client_id: &'static str,
    #[serde(rename = "AuthCode")]
    auth_code: &'a str,
    #[serde(rename = "CodeVerifier")]
    code_verifier: &'a str,
    #[serde(rename = "DeviceInfo")]
    device_info: DeviceInfo<'a>,
    #[serde(rename = "IDEVersion")]
    ide_version: &'a str,
}

#[derive(Serialize)]
struct UserInfoBody<'a> {
    #[serde(rename = "ReqSource")]
    req_source: &'static str,
    #[serde(rename = "IDEVersion")]
    ide_version: &'a str,
}

/// A plausible Linux workstation, fixed per credential by its machine ID.
/// iCubeSystemInformationService reports `osName` as `process.platform`,
/// `osVersion` as os-release PRETTY_NAME and the CPU brand before any "@";
/// `OPe` names the device after the user or, failing that, the hostname.
struct Persona {
    name: String,
    os_version: &'static str,
    cpu: &'static str,
}

const USERS: &[&str] = &[
    "alex", "chris", "dana", "jamie", "jordan", "kai", "lee", "morgan", "robin", "sam",
];
const MACHINES: &[&str] = &[
    "ThinkPad-X1-Carbon",
    "ThinkPad-T14",
    "XPS-15-9520",
    "XPS-13-9310",
    "Precision-5570",
    "OptiPlex-7090",
    "Latitude-7420",
    "desktop",
    "workstation",
];
const OS_VERSIONS: &[&str] = &[
    "Ubuntu 24.04.2 LTS",
    "Ubuntu 22.04.5 LTS",
    "Debian GNU/Linux 12 (bookworm)",
    "Fedora Linux 41 (Workstation Edition)",
    "Linux Mint 22.1",
];
const CPUS: &[&str] = &[
    "12th Gen Intel(R) Core(TM) i7-1260P",
    "13th Gen Intel(R) Core(TM) i7-13700H",
    "11th Gen Intel(R) Core(TM) i5-1145G7",
    "AMD Ryzen 7 5800X 8-Core Processor",
    "AMD Ryzen 7 PRO 6850U with Radeon Graphics",
    "AMD Ryzen 9 7950X 16-Core Processor",
];

fn persona(machine: &str) -> Persona {
    // FNV-1a: stable across builds and platforms, unlike the std hasher.
    let hash = machine
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let pick = |items: &'static [&'static str], shift: u32| {
        items[((hash >> shift) % items.len() as u64) as usize]
    };
    Persona {
        name: format!("{}-{}", pick(USERS, 0), pick(MACHINES, 16)),
        os_version: pick(OS_VERSIONS, 32),
        cpu: pick(CPUS, 48),
    }
}

fn device<'a>(
    config: &'a TraeConfig,
    id: &'a str,
    machine: &'a str,
    key: &SigningKey,
) -> Result<DeviceInfo<'a>, ChannelError> {
    let public = key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(invalid)?;
    let persona = persona(machine);
    Ok(DeviceInfo {
        device_id: id,
        machine_id: machine,
        platform_code: "IDE_PC",
        device_type: "PC",
        device_name: persona.name,
        device_model: "",
        client_version: config.version(),
        device_public_key: public,
        device_brand: "",
        device_cpu: persona.cpu,
        os_info: "linux",
        os_version: persona.os_version,
    })
}

fn encode(body: &impl Serialize) -> Result<Vec<u8>, ChannelError> {
    serde_json::to_vec(body).map_err(invalid)
}

/// marscode/request.js `getUserInfo`, as the desktop calls it after a login
/// and after every refresh.
async fn user_info(
    client: &dyn crate::OutboundClient,
    origin: &str,
    config: &TraeConfig,
    token: &str,
) -> Result<Value, ChannelError> {
    let (status, _, body) = send(
        client,
        Method::POST,
        &format!("{origin}/cloudide/api/v3/trae/GetUserInfo"),
        headers(token)?,
        Some(encode(&UserInfoBody {
            req_source: "IDE",
            ide_version: config.version(),
        })?),
    )
    .await?;
    let account = json(&require_success(status, body)?)?;
    check(&account)?;
    account
        .get("Result")
        .cloned()
        .ok_or_else(|| invalid("missing user information"))
}

/// The routing region: a US TTP login stays there, otherwise the account's
/// AI region, otherwise the login's region.
fn account_region(auth_region: Option<&str>, account: &Value) -> Option<String> {
    auth_region
        .filter(|r| *r == "USTTP")
        .or_else(|| account["AIRegion"].as_str().map(super::config::region))
        .or(auth_region)
        .map(str::to_owned)
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
    body: Vec<u8>,
    token: &str,
    refreshing: bool,
) -> Result<Value, ChannelError> {
    let (status, _, bytes) = send(
        client,
        Method::POST,
        &format!("{origin}{TOKEN_PATH}"),
        headers(token)?,
        Some(body),
    )
    .await?;
    // Only the native client's explicit invalid/revoked token codes are final
    // (marscode/request.js T$, forced logout on RefreshTokenInvalid).
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
        if matches!(
            code.as_deref(),
            Some("20324" | "20101" | "20315" | "20125" | "20126" | "20401" | "20403")
        ) {
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
    /// The desktop's verifier is `randomBytes(48)` base64url: 64 characters
    /// (marscode/util.js, main.js:52840).
    fn pkce_verifier_bytes(&self) -> usize {
        48
    }

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
            // login.js uses its own `randomUUID()` trace, not the CSRF state.
            let trace = {
                let hex = random_hex::<16>()?;
                let variant = ["8", "9", "a", "b"][usize::from(hex.as_bytes()[16] % 4)];
                format!(
                    "{}-{}-4{}-{variant}{}-{}",
                    &hex[..8],
                    &hex[8..12],
                    &hex[13..16],
                    &hex[17..20],
                    &hex[20..]
                )
            };
            let persona = persona(&machine);
            let mut url =
                url::Url::parse(&format!("{}/authorization", config.website())).map_err(invalid)?;
            // The desktop sends no `state`; ours stays (top level and inside the
            // callback) because `callback_result` checks it against the login.
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
                ("login_trace_id", trace.as_str()),
                ("machine_id", machine.as_str()),
                ("device_id", id.as_str()),
                ("x_device_id", id.as_str()),
                ("x_machine_id", machine.as_str()),
                // querystring.stringify keeps empty and undefined values.
                ("x_device_brand", ""),
                ("x_device_type", "linux"),
                ("x_os_version", persona.os_version),
                ("x_env", ""),
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
                encode(&AuthCodeBody {
                    client_id: CLIENT_ID,
                    auth_code: &code,
                    code_verifier: grant.code_verifier,
                    device_info: device(&config, id, machine, &key)?,
                    ide_version: config.version(),
                })?,
                "",
                false,
            )
            .await?;
            let token = result["Token"]
                .as_str()
                .ok_or(ChannelError::InvalidCredential)?;
            let account = user_info(context.client, origin, &config, token).await?;
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
                    account_region(region.as_deref(), &account).as_deref(),
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
                provider_secrets: BTreeMap::from([
                    ("device_key".into(), key_value.clone()),
                    (RELEASED_AT.into(), json!(now_ms())),
                ]),
            })
        })
    }
}

impl CredentialRefresh for OAuth {
    /// X8: refresh when a day or less remains, or less than a third of the
    /// token's lifetime. Credentials without a release time use the day rule.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        now_ms: i64,
    ) -> Option<RefreshReason> {
        let due = credential.expires_at_ms.is_some_and(|expires| {
            let remaining = expires.saturating_sub(now_ms);
            remaining <= REFRESH_WINDOW_MS
                || credential
                    .secret
                    .get("provider_secrets")
                    .and_then(|secrets| secrets.get(RELEASED_AT))
                    .and_then(Value::as_i64)
                    .is_some_and(|released| expires.saturating_sub(released) / 3 > remaining)
        }) || credential
            .secret
            .get("refresh_expires_at_ms")
            .and_then(Value::as_i64)
            .is_some_and(|expires| expires <= now_ms.saturating_add(REFRESH_WINDOW_MS));
        due.then_some(RefreshReason::ProactiveExpiry)
    }

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
            let body = encode(&RefreshBody {
                client_id: CLIENT_ID,
                client_secret: "",
                refresh_token: refresh,
                device_info: device(
                    &config,
                    fact(credential, "device_id").ok_or(ChannelError::InvalidCredential)?,
                    fact(credential, "machine_id").ok_or(ChannelError::InvalidCredential)?,
                    &key,
                )?,
                device_proof: DeviceProof {
                    signature: STANDARD.encode(signature.to_der()),
                    timestamp,
                    nonce,
                },
                ide_version: config.version(),
            })?;
            let auth_region = fact(credential, "auth_region");
            let origin = config.auth_origin(auth_region.or_else(|| fact(credential, "region")));
            let result = exchange(context.client, origin, body, token, true).await?;
            let expires_at_ms = expiry(&result);
            let mut secret = credential.secret.clone();
            if let Some(secrets) = secret
                .get_mut("provider_secrets")
                .and_then(Value::as_object_mut)
            {
                secrets.insert(RELEASED_AT.into(), json!(now_ms()));
            }
            secret["access_token"] = result["Token"].clone();
            if let Some(token) = result["RefreshToken"].as_str().filter(|s| !s.is_empty()) {
                secret["refresh_token"] = json!(token);
            }
            secret["expires_at_ms"] = json!(expires_at_ms);
            if result["RefreshExpireAt"].is_i64() {
                secret["refresh_expires_at_ms"] = result["RefreshExpireAt"].clone();
            }
            // OauthService.refreshToken re-reads the account with the new
            // token. The refreshed token is already spent upstream, so only an
            // account switch rejects; any other failure keeps the old facts.
            let fresh = result["Token"].as_str().unwrap_or_default();
            let mut metadata = None;
            if let Ok(account) = user_info(context.client, origin, &config, fresh).await {
                let user_id = account["UserID"].as_str().filter(|v| !v.is_empty());
                if let (Some(user_id), Some(known)) = (user_id, fact(credential, "user_id"))
                    && user_id != known
                {
                    return Err(ChannelError::RefreshRejected(
                        "Trae refresh returned a different account".into(),
                    ));
                }
                let mut fields = Vec::new();
                if let Some(region) = account_region(auth_region, &account) {
                    fields.push(("region", region));
                }
                if let Some(name) = account["ScreenName"].as_str() {
                    fields.push(("user_name", name.to_owned()));
                }
                if let Some(provider_fields) = secret
                    .get_mut("provider_fields")
                    .and_then(Value::as_object_mut)
                {
                    for (name, value) in &fields {
                        provider_fields.insert((*name).into(), json!(value));
                    }
                }
                // `fact` prefers metadata, so the public copy must move too.
                let mut public = credential.metadata.as_object().cloned().unwrap_or_default();
                for (name, value) in fields {
                    public.insert(name.into(), json!(value));
                }
                metadata = Some(Value::Object(public));
            }
            Ok(CredentialUpdate {
                metadata,
                secret,
                expires_at_ms,
            })
        })
    }
}
