use http::Method;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Entity {
    Organizations,
    Teams,
    Providers,
    Credentials,
    Routes,
    RouteMembers,
    Aliases,
    ModelAliases,
    Users,
    UserKeys,
    Permissions,
    RateLimits,
    Quotas,
    PriceRules,
    PriceRates,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Route {
    List(Entity),
    Create(Entity),
    Update(Entity, i64),
    Delete(Entity, i64),
    RevealUserKey(i64),
    Usage,
    QuotaWindows,
    CredentialCycles,
    Channels,
    TlsPresets,
    Audit,
}

pub(crate) fn parse(method: &Method, path: &str) -> Option<Route> {
    let segments = path.strip_prefix("/admin/")?.split('/').collect::<Vec<_>>();
    if segments.len() == 3
        && segments[0] == "user-keys"
        && segments[2] == "reveal"
        && method == Method::POST
    {
        return Some(Route::RevealUserKey(segments[1].parse().ok()?));
    }
    if segments.len() == 1 {
        return special(method, segments[0]).or_else(|| {
            let entity = entity(segments[0])?;
            match *method {
                Method::GET => Some(Route::List(entity)),
                Method::POST => Some(Route::Create(entity)),
                _ => None,
            }
        });
    }
    if segments.len() == 2 && method == Method::PATCH {
        return Some(Route::Update(
            entity(segments[0])?,
            segments[1].parse().ok()?,
        ));
    }
    if segments.len() == 2 && method == Method::DELETE {
        return Some(Route::Delete(
            entity(segments[0])?,
            segments[1].parse().ok()?,
        ));
    }
    None
}

fn special(method: &Method, name: &str) -> Option<Route> {
    match (method, name) {
        (&Method::GET, "usage") => Some(Route::Usage),
        (&Method::GET, "quota-windows") => Some(Route::QuotaWindows),
        (&Method::GET, "credential-cycles") => Some(Route::CredentialCycles),
        (&Method::GET, "channels") => Some(Route::Channels),
        (&Method::GET, "tls-presets") => Some(Route::TlsPresets),
        (&Method::GET, "audit") => Some(Route::Audit),
        _ => None,
    }
}

fn entity(name: &str) -> Option<Entity> {
    Some(match name {
        "organizations" => Entity::Organizations,
        "teams" => Entity::Teams,
        "providers" => Entity::Providers,
        "credentials" => Entity::Credentials,
        "routes" => Entity::Routes,
        "route-members" => Entity::RouteMembers,
        "aliases" => Entity::Aliases,
        "model-aliases" => Entity::ModelAliases,
        "users" => Entity::Users,
        "user-keys" => Entity::UserKeys,
        "permissions" => Entity::Permissions,
        "rate-limits" => Entity::RateLimits,
        "quotas" => Entity::Quotas,
        "price-rules" => Entity::PriceRules,
        "price-rates" => Entity::PriceRates,
        _ => return None,
    })
}
