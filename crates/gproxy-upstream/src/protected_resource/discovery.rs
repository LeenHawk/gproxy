//! From a resource URL to the authorization server that guards it.

use gproxy_client::OutboundClient;
use http::{HeaderMap, Method, StatusCode, header};
use serde::Deserialize;
use url::Url;

use super::http::{get_json, send};
use crate::UpstreamError;

/// The parts of a `WWW-Authenticate: Bearer` challenge discovery reads.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Challenge {
    pub resource_metadata: Option<String>,
    pub scope: Option<String>,
}

impl Challenge {
    pub fn parse(header: &str) -> Self {
        Self {
            resource_metadata: parameter(header, "resource_metadata"),
            scope: parameter(header, "scope"),
        }
    }
}

/// One `name="value"` or `name=token` auth-param of a challenge.
fn parameter(header: &str, name: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let start = from + found;
        let end = start + name.len();
        from = end;
        let boundary = start == 0 || matches!(header.as_bytes()[start - 1], b' ' | b',' | b'\t');
        let Some(value) = header[end..].trim_start().strip_prefix('=') else {
            continue;
        };
        if !boundary {
            continue;
        }
        let value = value.trim_start();
        return Some(match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next()?.to_owned(),
            None => value
                .split(|c: char| c == ',' || c.is_whitespace())
                .next()?
                .to_owned(),
        })
        .filter(|value| !value.is_empty());
    }
    None
}

/// RFC 8414 / OpenID Connect Discovery metadata, the fields a sign-in uses.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizationServer {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Discovery {
    /// The `resource` indicator to request tokens for.
    pub resource: String,
    pub authorization_server: AuthorizationServer,
    /// What to ask for: the challenge's scope, else what the resource lists.
    pub scope: Option<String>,
}

#[derive(Deserialize)]
struct ResourceMetadata {
    resource: Option<String>,
    #[serde(default)]
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

fn parse(url: &str) -> Result<Url, UpstreamError> {
    Url::parse(url).map_err(|error| UpstreamError::InvalidConfig(format!("{url}: {error}")))
}

/// `/.well-known/{name}` inserted before the URL's own path, then at the root.
fn well_known(url: &Url, name: &str) -> Vec<String> {
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    let mut candidates = Vec::new();
    if !path.is_empty() {
        candidates.push(format!("{origin}/.well-known/{name}{path}"));
    }
    candidates.push(format!("{origin}/.well-known/{name}"));
    candidates
}

/// Ask the resource without a token; a 401 says where its metadata lives.
pub(super) async fn probe(
    client: &dyn OutboundClient,
    resource: &str,
) -> Result<Option<String>, UpstreamError> {
    let (status, headers, _) = send(client, Method::GET, resource, HeaderMap::new(), None).await?;
    Ok((status == StatusCode::UNAUTHORIZED)
        .then(|| headers.get(header::WWW_AUTHENTICATE)?.to_str().ok())
        .flatten()
        .map(str::to_owned))
}

/// The metadata's `resource` has to cover the URL it was found for.
fn covers(declared: &str, resource: &Url) -> bool {
    Url::parse(declared).is_ok_and(|declared| {
        declared.origin() == resource.origin()
            && resource
                .path()
                .trim_end_matches('/')
                .starts_with(declared.path().trim_end_matches('/'))
    })
}

/// Find the authorization server of `resource`. `challenge` is the
/// `WWW-Authenticate` value of a 401 from it, when there was one.
pub async fn discover(
    client: &dyn OutboundClient,
    resource: &str,
    challenge: Option<&str>,
) -> Result<Discovery, UpstreamError> {
    let resource_url = parse(resource)?;
    let challenge = challenge.map(Challenge::parse).unwrap_or_default();
    let mut candidates = well_known(&resource_url, "oauth-protected-resource");
    if let Some(url) = challenge.resource_metadata {
        candidates.insert(0, url);
    }
    let mut metadata = None;
    for url in &candidates {
        if let Some(document) = get_json(client, url).await? {
            metadata = serde_json::from_value::<ResourceMetadata>(document).ok();
            break;
        }
    }

    let mut indicator = resource.to_owned();
    let mut scopes = Vec::new();
    // A resource that publishes no metadata is its own authorization server,
    // as MCP servers older than 2025-06-18 are.
    let mut issuer = resource_url.origin().ascii_serialization();
    if let Some(metadata) = metadata {
        if let Some(declared) = metadata.resource {
            if !covers(&declared, &resource_url) {
                return Err(UpstreamError::InvalidResponse(format!(
                    "resource metadata describes `{declared}`, not `{resource}`"
                )));
            }
            indicator = declared;
        }
        if let Some(first) = metadata.authorization_servers.into_iter().next() {
            issuer = first;
        }
        scopes = metadata.scopes_supported;
    }

    let authorization_server = authorization_server(client, &issuer).await?;
    if authorization_server
        .code_challenge_methods_supported
        .as_ref()
        .is_some_and(|methods| !methods.iter().any(|method| method == "S256"))
    {
        return Err(UpstreamError::InvalidResponse(format!(
            "authorization server `{issuer}` does not support PKCE with S256"
        )));
    }
    let mut scope = challenge
        .scope
        .or_else(|| (!scopes.is_empty()).then(|| scopes.join(" ")));
    // Without it most servers issue no refresh token.
    if let Some(scope) = &mut scope
        && authorization_server
            .scopes_supported
            .iter()
            .any(|supported| supported == "offline_access")
        && !scope.split(' ').any(|asked| asked == "offline_access")
    {
        scope.push_str(" offline_access");
    }
    Ok(Discovery {
        resource: indicator,
        authorization_server,
        scope,
    })
}

async fn authorization_server(
    client: &dyn OutboundClient,
    issuer: &str,
) -> Result<AuthorizationServer, UpstreamError> {
    let url = parse(issuer)?;
    let mut candidates = well_known(&url, "oauth-authorization-server");
    candidates.truncate(1);
    candidates.extend(well_known(&url, "openid-configuration").into_iter().take(1));
    let path = url.path().trim_end_matches('/');
    if !path.is_empty() {
        candidates.push(format!(
            "{}{path}/.well-known/openid-configuration",
            url.origin().ascii_serialization()
        ));
    }
    for candidate in &candidates {
        let Some(document) = get_json(client, candidate).await? else {
            continue;
        };
        let server: AuthorizationServer = serde_json::from_value(document)
            .map_err(|error| UpstreamError::InvalidResponse(format!("{candidate}: {error}")))?;
        if server.issuer.trim_end_matches('/') != issuer.trim_end_matches('/') {
            return Err(UpstreamError::InvalidResponse(format!(
                "{candidate} is the metadata of `{}`, not `{issuer}`",
                server.issuer
            )));
        }
        return Ok(server);
    }
    Err(UpstreamError::InvalidResponse(format!(
        "no authorization server metadata found for `{issuer}`"
    )))
}

#[cfg(test)]
mod tests {
    use super::Challenge;

    #[test]
    fn a_challenge_yields_its_metadata_url_and_scope() {
        assert_eq!(
            Challenge::parse(
                r#"Bearer realm="mcp", error="invalid_token", resource_metadata="https://r.example/.well-known/oauth-protected-resource/mcp", scope="read write""#
            ),
            Challenge {
                resource_metadata: Some(
                    "https://r.example/.well-known/oauth-protected-resource/mcp".to_owned()
                ),
                scope: Some("read write".to_owned()),
            }
        );
        assert_eq!(
            Challenge::parse("Bearer scope=read"),
            Challenge {
                resource_metadata: None,
                scope: Some("read".to_owned()),
            }
        );
        assert_eq!(
            Challenge::parse(r#"Bearer error="insufficient_scope""#),
            Challenge::default()
        );
    }
}
