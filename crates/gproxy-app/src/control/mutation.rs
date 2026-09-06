use crate::{AppError, AppHandle};

pub enum ControlMutation {
    Provider(gproxy_store::records::ProviderInput),
    Credential {
        provider_id: i64,
        label: Option<String>,
        secret: serde_json::Value,
        enabled: bool,
    },
    Route(gproxy_store::records::RouteInput),
    RouteMember(gproxy_store::records::RouteMemberInput),
    Alias(gproxy_store::records::AliasInput),
    ExposedModel(gproxy_store::records::ExposedModelInput),
    Organization(gproxy_store::records::OrganizationInput),
    Team(gproxy_store::records::TeamInput),
    User(gproxy_store::records::UserInput),
    UserKey {
        user_id: i64,
        api_key: String,
        label: Option<String>,
        expires_at: Option<i64>,
        enabled: bool,
    },
    Permission(gproxy_store::records::PermissionInput),
    RateLimit(gproxy_store::records::RateLimitInput),
    Quota(gproxy_store::records::QuotaInput),
    PriceRule(gproxy_store::records::PriceRuleInput),
    PriceRate(gproxy_store::records::PriceRateInput),
    Setting(gproxy_store::records::SettingInput),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationResult {
    Id(i64),
    Applied,
}

pub(crate) async fn apply(
    handle: &AppHandle,
    mutation: ControlMutation,
) -> Result<MutationResult, AppError> {
    let services = &handle.inner.host.services;
    let result = match mutation {
        ControlMutation::Provider(input) => {
            let id = services.store.insert_provider(&input).await?;
            gproxy_admin::seed_provider_rule_set(&services.store, id, &input.name)
                .await
                .map_err(|error| AppError::Control(error.to_string()))?;
            MutationResult::Id(id)
        }
        ControlMutation::Credential {
            provider_id,
            label,
            secret,
            enabled,
        } => {
            let label =
                label.or_else(|| gproxy_admin::default_credential_label("api_key", &secret));
            let envelope = services.cipher.seal(&secret)?;
            MutationResult::Id(
                services
                    .store
                    .insert_credential(&gproxy_store::records::CredentialInput {
                        provider_id,
                        label,
                        kind: "api_key".into(),
                        envelope,
                        enabled,
                        weight: 100,
                        rpm_limit: None,
                        tpm_limit: None,
                        proxy_url: None,
                        tls_fingerprint: None,
                    })
                    .await?,
            )
        }
        ControlMutation::Route(input) => {
            nonzero(input.max_attempts, "route max_attempts")?;
            MutationResult::Id(services.store.insert_route(&input).await?)
        }
        ControlMutation::RouteMember(input) => {
            MutationResult::Id(services.store.insert_route_member(&input).await?)
        }
        ControlMutation::Alias(input) => {
            MutationResult::Id(services.store.insert_alias(&input).await?)
        }
        ControlMutation::ExposedModel(input) => {
            MutationResult::Id(services.store.insert_exposed_model(&input).await?)
        }
        ControlMutation::Organization(input) => {
            MutationResult::Id(services.store.insert_organization(&input).await?)
        }
        ControlMutation::Team(input) => {
            MutationResult::Id(services.store.insert_team(&input).await?)
        }
        ControlMutation::User(input) => {
            MutationResult::Id(services.store.insert_user(&input).await?)
        }
        ControlMutation::UserKey {
            user_id,
            api_key,
            label,
            expires_at,
            enabled,
        } => {
            if api_key.is_empty() {
                return Err(AppError::Control("API key must not be empty".into()));
            }
            let envelope = services
                .cipher
                .seal_user_key(&serde_json::Value::String(api_key.clone()))?;
            MutationResult::Id(
                services
                    .store
                    .insert_user_key(&gproxy_store::records::UserKeyInput {
                        user_id,
                        digest: super::user_key_digest(super::USER_KEY_DIGEST_VERSION, &api_key)
                            .expect("current user-key digest version is supported"),
                        digest_version: super::USER_KEY_DIGEST_VERSION,
                        prefix: api_key.chars().take(12).collect(),
                        envelope,
                        label,
                        expires_at,
                        enabled,
                    })
                    .await?,
            )
        }
        ControlMutation::Permission(input) => {
            MutationResult::Id(services.store.insert_permission(&input).await?)
        }
        ControlMutation::RateLimit(input) => {
            nonzero(input.window_seconds, "rate limit window_seconds")?;
            MutationResult::Id(services.store.insert_rate_limit(&input).await?)
        }
        ControlMutation::Quota(input) => {
            validate_quota(&input)?;
            MutationResult::Id(services.store.insert_quota(&input).await?)
        }
        ControlMutation::PriceRule(input) => {
            MutationResult::Id(services.store.insert_price_rule(&input).await?)
        }
        ControlMutation::PriceRate(input) => {
            nonzero(input.unit_size, "price rate unit_size")?;
            MutationResult::Id(services.store.insert_price_rate(&input).await?)
        }
        ControlMutation::Setting(input) => {
            validate_setting(&input)?;
            services.store.set_setting(&input).await?;
            MutationResult::Applied
        }
    };
    handle.reload().await?;
    Ok(result)
}

fn validate_setting(input: &gproxy_store::records::SettingInput) -> Result<(), AppError> {
    if !matches!(
        input.key.as_str(),
        crate::cleanup::RETENTION_DAYS | crate::cleanup::MAX_DATABASE_SIZE_MB
    ) {
        return Ok(());
    }
    let Some(value) = input.value.as_i64() else {
        return Err(AppError::Control(format!(
            "{} must be an integer; non-positive disables it",
            input.key
        )));
    };
    let maximum = if input.key == crate::cleanup::RETENTION_DAYS {
        i64::MAX / 86_400
    } else {
        i64::try_from(u64::MAX / (1024 * 1024)).expect("MiB limit fits i64")
    };
    if value > maximum {
        return Err(AppError::Control(format!(
            "{} exceeds its supported range",
            input.key
        )));
    }
    Ok(())
}

fn nonzero(value: impl Into<u64>, field: &'static str) -> Result<(), AppError> {
    if value.into() == 0 {
        Err(AppError::Control(format!("{field} must be positive")))
    } else {
        Ok(())
    }
}

fn validate_quota(input: &gproxy_store::records::QuotaInput) -> Result<(), AppError> {
    for (field, value) in [
        ("quota_total", input.quota_total),
        ("quota_daily", input.quota_daily),
        ("quota_weekly", input.quota_weekly),
        ("quota_monthly", input.quota_monthly),
        ("quota_5h", input.quota_5h),
        ("quota_7d", input.quota_7d),
    ] {
        if value.is_some_and(|value| {
            value < rust_decimal::Decimal::ZERO
                || (input.subject_kind != "credential" && value == rust_decimal::Decimal::ZERO)
        }) {
            return Err(AppError::Control(format!("{field} must be positive")));
        }
    }
    Ok(())
}
