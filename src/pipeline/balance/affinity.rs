use std::sync::Arc;
use std::time::Duration;

use http::HeaderMap;
use serde_json::Value;

use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Route, RouteMember};

const AFFINITY_TTL: Duration = Duration::from_secs(3600);
const SESSION_HEADER: &str = "x-gproxy-session-id";

pub(crate) fn take_session_id(headers: &mut HeaderMap) -> Option<String> {
    headers
        .remove(SESSION_HEADER)
        .and_then(|value| value.to_str().ok().map(str::trim).map(str::to_owned))
        .filter(|value| !value.is_empty())
}

pub(super) fn member_key(
    route: &Route,
    user_key_id: Option<i64>,
    session_id: Option<&str>,
) -> Option<Arc<str>> {
    if !enabled(route.settings_json.as_ref()) {
        return None;
    }
    let subject = match (user_key_id, session_id.filter(|value| !value.is_empty())) {
        (Some(user_id), Some(session_id)) => format!(
            "user:{user_id}:session:{}",
            blake3::hash(session_id.as_bytes()).to_hex()
        ),
        (Some(user_id), None) => format!("user:{user_id}"),
        (None, Some(session_id)) => {
            format!("session:{}", blake3::hash(session_id.as_bytes()).to_hex())
        }
        (None, None) => return None,
    };
    Some(format!("route_aff:{}:{subject}", route.id).into())
}

fn enabled(settings: Option<&Value>) -> bool {
    settings
        .and_then(|settings| settings.get("affinity"))
        .and_then(|affinity| affinity.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(super) async fn read_pin(cache: &dyn CacheBackend, key: Option<&str>) -> Option<i64> {
    let key = key?;
    cache
        .get(key)
        .await
        .and_then(|value| String::from_utf8(value).ok())
        .and_then(|value| value.parse().ok())
}

pub(super) async fn write_pin(cache: &dyn CacheBackend, key: &str, id: i64) {
    let _ = cache
        .set(key, id.to_string().into_bytes(), Some(AFFINITY_TTL))
        .await;
}

pub(super) fn prefer_member(members: &mut Vec<&RouteMember>, pinned: Option<i64>) {
    let Some(pinned) = pinned else {
        return;
    };
    let Some(position) = members.iter().position(|member| member.id == pinned) else {
        return;
    };
    if position > 0 {
        let member = members.remove(position);
        members.insert(0, member);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn route(settings_json: Option<Value>) -> Route {
        Route {
            id: 7,
            name: "r".into(),
            strategy: "round_robin".into(),
            enabled: true,
            description: None,
            settings_json,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn session_key_overrides_user_key() {
        let route = route(Some(json!({ "affinity": { "enabled": true } })));
        let user = member_key(&route, Some(9), None).expect("user affinity");
        let session = member_key(&route, Some(9), Some("chat-1")).expect("session affinity");
        assert_eq!(user.as_ref(), "route_aff:7:user:9");
        assert!(session.starts_with("route_aff:7:user:9:session:"));
        assert_ne!(session, user);
        assert_ne!(
            session,
            member_key(&route, Some(10), Some("chat-1")).expect("other user session")
        );
    }

    #[test]
    fn affinity_is_opt_in() {
        assert!(member_key(&route(None), Some(9), Some("chat-1")).is_none());
    }
}
