use sea_query::{Alias, Expr, ExprTrait, JoinType, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::{
    common::{insert_select, value},
    oauth_clients,
};
use crate::records::{OAuthAuthorizationInput, OAuthDeviceInput};

pub(crate) fn create(input: &OAuthAuthorizationInput) -> Result<Vec<Statement>, StoreError> {
    let mut user = Query::select();
    user.column(Alias::new("id"))
        .from(Alias::new("users"))
        .and_where(Expr::col(Alias::new("id")).eq(input.key.user_id))
        .and_where(Expr::col(Alias::new("enabled")).eq(1));
    let mut key = oauth_clients::enabled(&input.client_id);
    key.and_where(Expr::exists(user)).exprs([
        value(input.key.user_id),
        value(input.key.digest.clone()),
        value(i64::from(input.key.digest_version)),
        value(input.key.prefix.clone()),
        value(input.key.label.clone()),
        value(input.key.expires_at),
        value(1_i64),
        value(input.key.envelope.ciphertext.clone()),
        value(input.key.envelope.wrapped_key.clone()),
        value(input.key.envelope.payload_nonce.clone()),
        value(input.key.envelope.key_nonce.clone()),
    ]);
    let mut grant = Query::select();
    grant
        .from(Alias::new("user_keys"))
        .and_where(Expr::col(Alias::new("digest")).eq(input.key.digest.clone()))
        .and_where(Expr::col(Alias::new("digest_version")).eq(i64::from(input.key.digest_version)))
        .exprs([
            value(input.key.user_id),
            Expr::col(Alias::new("id")),
            value(input.provider_id),
            value(input.client_id.clone()),
            value(input.scopes.clone()),
            value(input.chatgpt_user_id.clone()),
            value(input.chatgpt_account_id.clone()),
            value(input.created_at),
        ]);
    let mut code = Query::select();
    code.from(Alias::new("oauth_grants"))
        .join(
            JoinType::InnerJoin,
            Alias::new("user_keys"),
            Expr::col((Alias::new("oauth_grants"), Alias::new("user_key_id")))
                .equals((Alias::new("user_keys"), Alias::new("id"))),
        )
        .and_where(
            Expr::col((Alias::new("user_keys"), Alias::new("digest"))).eq(input.key.digest.clone()),
        )
        .and_where(
            Expr::col((Alias::new("user_keys"), Alias::new("digest_version")))
                .eq(i64::from(input.key.digest_version)),
        )
        .exprs([
            value(input.code_digest.clone()),
            Expr::col((Alias::new("oauth_grants"), Alias::new("id"))),
            value(input.redirect_uri.clone()),
            value(input.code_challenge.clone()),
            value(input.created_at),
            value(input.expires_at),
        ]);
    Ok(vec![
        oauth_clients::lock(&input.client_id)?,
        insert_select(
            "user_keys",
            &[
                "user_id",
                "digest",
                "digest_version",
                "prefix",
                "label",
                "expires_at",
                "enabled",
                "ciphertext",
                "wrapped_key",
                "payload_nonce",
                "key_nonce",
            ],
            key,
        )?,
        insert_select(
            "oauth_grants",
            &[
                "user_id",
                "user_key_id",
                "provider_id",
                "client_id",
                "scopes",
                "chatgpt_user_id",
                "chatgpt_account_id",
                "created_at",
            ],
            grant,
        )?,
        insert_select(
            "oauth_codes",
            &[
                "digest",
                "grant_id",
                "redirect_uri",
                "code_challenge",
                "created_at",
                "expires_at",
            ],
            code,
        )?,
    ])
}

pub(crate) fn device(input: &OAuthDeviceInput) -> Result<Vec<Statement>, StoreError> {
    let mut select = oauth_clients::enabled(&input.client_id);
    select.exprs([
        value(input.device_digest.clone()),
        value(input.user_code.clone()),
        value(input.client_id.clone()),
        value(input.provider_id),
        value(input.created_at),
        value(input.expires_at),
    ]);
    Ok(vec![
        oauth_clients::lock(&input.client_id)?,
        insert_select(
            "oauth_devices",
            &[
                "device_digest",
                "user_code",
                "client_id",
                "provider_id",
                "created_at",
                "expires_at",
            ],
            select,
        )?,
    ])
}
