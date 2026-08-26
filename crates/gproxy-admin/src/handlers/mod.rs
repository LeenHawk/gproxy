mod audit;
mod catalogue;
mod control;
mod identity;
pub(crate) mod login;
pub(crate) mod observability;
mod portal_settings;
mod pricing;
mod util;

use bytes::Bytes;
use http::Response;
use http::request::Parts;

use crate::auth::AdminIdentity;
use crate::route::{Entity, Route};
use crate::{AdminError, State};

pub(crate) async fn dispatch(
    state: &impl State,
    admin: &AdminIdentity,
    route: Route,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    match route {
        Route::List(entity) => list(state, entity).await,
        Route::Create(entity) => create(state, entity, body).await,
        Route::Update(entity, id) => update(state, entity, id, body).await,
        Route::Delete(entity, id) => delete(state, entity, id).await,
        Route::RevealUserKey(id) => identity::reveal(state, admin, id).await,
        Route::Usage => observability::usage(state, parts).await,
        Route::QuotaWindows => observability::quota_windows(state, parts).await,
        Route::CredentialCycles => observability::credential_cycles(state, parts).await,
        Route::Channels => catalogue::channels(state),
        Route::TlsPresets => catalogue::tls_presets(),
        Route::Audit => audit::list(state, parts).await,
        Route::PortalSettingsRead => portal_settings::get(state).await,
        Route::PortalSettingsWrite => portal_settings::update(state, body).await,
        Route::LoginAuthCodeStart => login::authcode_start(state, body).await,
        Route::LoginAuthCodeComplete => login::authcode_complete(state, body).await,
        Route::LoginDeviceStart => login::device_start(state, body).await,
        Route::LoginDevicePoll => login::device_poll(state, body).await,
        Route::LoginCookieExchange => login::cookie_exchange(state, body).await,
    }
}

async fn delete(
    state: &impl State,
    entity: Entity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    match entity {
        Entity::Permissions | Entity::RateLimits | Entity::Quotas => {
            identity::delete(state, entity, id).await
        }
        Entity::PriceRates => pricing::delete(state, id).await,
        _ => Err(AdminError::NotFound),
    }
}

async fn list(state: &impl State, entity: Entity) -> Result<Response<Bytes>, AdminError> {
    match entity {
        Entity::Providers
        | Entity::Credentials
        | Entity::Routes
        | Entity::RouteMembers
        | Entity::Aliases
        | Entity::ModelAliases => control::list(state, entity).await,
        Entity::Organizations
        | Entity::Teams
        | Entity::Users
        | Entity::UserKeys
        | Entity::Permissions
        | Entity::RateLimits
        | Entity::Quotas => identity::list(state, entity).await,
        Entity::PriceRules | Entity::PriceRates => pricing::list(state, entity).await,
    }
}

async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    match entity {
        Entity::Providers
        | Entity::Credentials
        | Entity::Routes
        | Entity::RouteMembers
        | Entity::Aliases
        | Entity::ModelAliases => control::create(state, entity, body).await,
        Entity::Organizations
        | Entity::Teams
        | Entity::Users
        | Entity::UserKeys
        | Entity::Permissions
        | Entity::RateLimits
        | Entity::Quotas => identity::create(state, entity, body).await,
        Entity::PriceRules | Entity::PriceRates => pricing::create(state, entity, body).await,
    }
}

async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    match entity {
        Entity::Providers
        | Entity::Credentials
        | Entity::Routes
        | Entity::RouteMembers
        | Entity::Aliases
        | Entity::ModelAliases => control::update(state, entity, id, body).await,
        Entity::Organizations
        | Entity::Teams
        | Entity::Users
        | Entity::UserKeys
        | Entity::Permissions
        | Entity::RateLimits
        | Entity::Quotas => identity::update(state, entity, id, body).await,
        Entity::PriceRules | Entity::PriceRates => pricing::update(state, entity, id, body).await,
    }
}
