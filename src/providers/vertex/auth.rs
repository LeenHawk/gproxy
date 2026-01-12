use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::context::AppContext;
use crate::providers::credential_status::now_timestamp;
use crate::providers::vertex::VertexCredential;

const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const TOKEN_TTL_SECS: i64 = 60 * 60;
const TOKEN_SKEW_SECS: i64 = 30;

#[derive(Serialize)]
struct ServiceAccountClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

pub(crate) async fn ensure_access_token(
    ctx: &AppContext,
    credential: &VertexCredential,
) -> Result<String, StatusCode> {
    let now = now_timestamp();
    if !credential.access_token.is_empty() && credential.expires_at > now + TOKEN_SKEW_SECS {
        return Ok(credential.access_token.clone());
    }

    let (token, expires_at) = fetch_service_account_token(ctx, credential, now).await?;
    let project_id = credential.project_id.clone();
    let token_clone = token.clone();
    ctx.vertex()
        .update_credential_by_id(&project_id, move |cred| {
            cred.access_token = token_clone.clone();
            cred.expires_at = expires_at;
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(token)
}

async fn fetch_service_account_token(
    ctx: &AppContext,
    credential: &VertexCredential,
    now: i64,
) -> Result<(String, i64), StatusCode> {
    if credential.client_email.trim().is_empty() || credential.private_key.trim().is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let header = Header::new(Algorithm::RS256);
    let claims = ServiceAccountClaims {
        iss: credential.client_email.clone(),
        scope: OAUTH_SCOPE.to_string(),
        aud: credential.token_uri.clone(),
        exp: now + TOKEN_TTL_SECS,
        iat: now,
    };
    let key = EncodingKey::from_rsa_pem(credential.private_key.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let assertion = encode(&header, &claims, &key).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
        .append_pair("assertion", &assertion)
        .finish();

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );

    let res = ctx
        .http_client()
        .post(credential.token_uri.as_str())
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = res.status();
    let body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !status.is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let token: TokenResponse =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_GATEWAY)?;
    let expires_in = token.expires_in.unwrap_or(TOKEN_TTL_SECS);
    let expires_at = (now + expires_in - TOKEN_SKEW_SECS).max(now);
    Ok((token.access_token, expires_at))
}
