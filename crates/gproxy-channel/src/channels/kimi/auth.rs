//! Which of the two accounts a credential is, where it talks, and the CLI
//! identity a subscription credential must present.

use super::config::{
    CLI_PLATFORM, CLI_USER_AGENT_PRODUCT, DEFAULT_API_BASE_URL, DEFAULT_CODE_BASE_URL, KimiConfig,
};
use crate::channel::{ChannelError, CredentialView, ProviderView};
use http::{HeaderMap, HeaderValue, header};
use serde_json::Value;

/// The two things a Kimi credential can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A Moonshot platform key, billed per token.
    ApiKey,
    /// A Kimi Code subscription reached through the CLI's device login.
    Subscription,
}

/// A credential that carries an OAuth token is a subscription, whatever the
/// host recorded its `auth_kind` as; a bare `api_key` is a platform key.
pub fn mode(credential: &CredentialView<'_>) -> Mode {
    let has_token = credential
        .secret
        .get("access_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty());
    if credential.auth_kind == "oauth" || has_token {
        Mode::Subscription
    } else {
        Mode::ApiKey
    }
}

/// Provider column, then the origin the login recorded, then the default for
/// this mode. The two products are different hosts, not one host with two
/// paths.
pub(super) fn base_url(
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
    mode: Mode,
) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            credential
                .secret
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            match mode {
                Mode::ApiKey => DEFAULT_API_BASE_URL,
                Mode::Subscription => DEFAULT_CODE_BASE_URL,
            }
            .to_owned()
        })
        .trim_end_matches('/')
        .to_owned()
}

/// A subscription origin already ends in `/coding/v1`, so the caller's own
/// `/v1` prefix would be doubled.
pub(super) fn path(mode: Mode, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    match mode {
        Mode::ApiKey => path,
        Mode::Subscription => path
            .strip_prefix("/v1/")
            .map(|rest| format!("/{rest}"))
            .unwrap_or(path),
    }
}

/// The token this mode authenticates with.
pub(super) fn token<'a>(
    credential: &CredentialView<'a>,
    mode: Mode,
) -> Result<&'a str, ChannelError> {
    let field = match mode {
        Mode::ApiKey => "api_key",
        Mode::Subscription => "access_token",
    };
    credential
        .secret
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

/// The device identity the login generated, wherever the host filed it.
pub(super) fn device_id(credential: &CredentialView<'_>) -> Option<String> {
    for value in [
        credential.secret.get("device_id"),
        credential.secret.pointer("/provider_fields/device_id"),
        credential.metadata.get("device_id"),
    ] {
        if let Some(id) = value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            return Some(id.to_owned());
        }
    }
    None
}

/// Header names the channel owns: a client may not claim to be the CLI.
pub(super) const IDENTITY_HEADERS: &[&str] = &[
    "user-agent",
    "x-msh-platform",
    "x-msh-version",
    "x-msh-device-name",
    "x-msh-device-model",
    "x-msh-os-version",
    "x-msh-device-id",
];

/// Header values carry no control characters and no non-ASCII; an empty or
/// unusable value becomes `unknown` rather than failing the request.
fn sanitized(value: Option<&str>) -> String {
    let cleaned: String = value
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// What the Kimi Code CLI sends alongside every subscription request. The
/// device id is required: the upstream ties a subscription to one machine.
pub(super) fn identity(
    headers: &mut HeaderMap,
    config: &KimiConfig,
    credential: &CredentialView<'_>,
) -> Result<(), ChannelError> {
    let device_id = device_id(credential).ok_or(ChannelError::InvalidCredential)?;
    identity_with_device(headers, config, &device_id)
}

/// Node's `os.arch()` spelling of the build's architecture; a proxy built for
/// anything else (the Workers target included) claims the common one.
fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    }
}

/// Kernel releases `os.release()` reports on current Ubuntu LTS installs; one
/// is picked per device so a machine keeps its kernel across requests.
const KERNEL_RELEASES: &[&str] = &[
    "6.8.0-60-generic",
    "6.8.0-79-generic",
    "6.11.0-26-generic",
    "6.14.0-29-generic",
];
/// Stems for a default `os.hostname()`, likewise picked per device.
const HOST_STEMS: &[&str] = &["ubuntu", "devbox", "workstation", "desktop"];

/// A stable number for a device id (FNV-1a), so every default a device id
/// implies is the same on every request and every proxy instance.
fn device_seed(device_id: &str) -> u64 {
    device_id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn pick<'a>(choices: &[&'a str], seed: u64) -> &'a str {
    choices[(seed % choices.len() as u64) as usize]
}

/// The same headers for a login, which has minted a device id but has no
/// credential to read it back from yet.
///
/// The CLI reads them off the machine (kimi-code `identity.ts`
/// `createKimiDeviceHeaders`): `hostname()`, `release()`, and a model of
/// `${type()} ${release()} ${arch()}`. A proxy is not that machine, so the
/// operator may state each; what is left unstated is a Linux machine derived
/// from the device id, the shape a real CLI on Linux would send, and stable
/// for as long as the credential is.
pub(super) fn identity_with_device(
    headers: &mut HeaderMap,
    config: &KimiConfig,
    device_id: &str,
) -> Result<(), ChannelError> {
    let seed = device_seed(device_id);
    let os_version = config
        .os_version
        .clone()
        .unwrap_or_else(|| pick(KERNEL_RELEASES, seed).to_owned());
    let model = config
        .device_model
        .clone()
        .unwrap_or_else(|| format!("Linux {} {}", sanitized(Some(&os_version)), node_arch()));
    let name = config
        .device_name
        .clone()
        .unwrap_or_else(|| format!("{}-{:04x}", pick(HOST_STEMS, seed), (seed >> 48) as u16));
    let version = sanitized(Some(&config.cli_version));
    for (name, value) in [
        // The product token is the CLI's own name, not its platform id
        // (kimi-code `constant/app.ts` `CLI_USER_AGENT_PRODUCT`).
        ("user-agent", format!("{CLI_USER_AGENT_PRODUCT}/{version}")),
        ("x-msh-platform", CLI_PLATFORM.to_owned()),
        ("x-msh-version", version.clone()),
        ("x-msh-device-name", sanitized(Some(&name))),
        ("x-msh-device-model", sanitized(Some(&model))),
        ("x-msh-os-version", sanitized(Some(&os_version))),
        ("x-msh-device-id", sanitized(Some(device_id))),
    ] {
        headers.insert(
            http::HeaderName::from_static(name),
            HeaderValue::from_str(&value).map_err(|_| ChannelError::InvalidCredential)?,
        );
    }
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscription_origin_absorbs_the_callers_v1() {
        assert_eq!(
            path(Mode::ApiKey, "/v1/chat/completions"),
            "/v1/chat/completions"
        );
        assert_eq!(
            path(Mode::Subscription, "/v1/chat/completions"),
            "/chat/completions"
        );
        assert_eq!(path(Mode::Subscription, "v1/messages"), "/messages");
        assert_eq!(path(Mode::Subscription, "/usages"), "/usages");
    }

    #[test]
    fn identity_values_are_ascii_and_never_empty() {
        assert_eq!(sanitized(Some("  my-host \n")), "my-host");
        assert_eq!(sanitized(Some("主机")), "unknown");
        assert_eq!(sanitized(None), "unknown");
    }
}
