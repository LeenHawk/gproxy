use std::sync::Arc;
use std::time::Duration;

use http::HeaderMap;
use serde_json::Value;

use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Route, RouteMember};

const AFFINITY_TTL: Duration = Duration::from_secs(3600);
const SESSION_HEADER: &str = "x-gproxy-session-id";
const REANCHOR_SUFFIX: &str = ":reanchor";
const MAX_REANCHOR_SECS: u64 = 365 * 24 * 60 * 60;

/// Request-scoped route-member affinity state shared by every candidate.
///
/// The main pin keeps its legacy integer value and rolling one-hour TTL.  When
/// hard re-anchoring is configured, `anchored_member` is populated only when
/// both the main pin and its fixed-age marker agree with the current interval.
/// A successful different member (including any request whose marker was
/// absent/expired/invalid) establishes a fresh marker.
#[derive(Debug)]
pub(crate) struct MemberAffinityPlan {
    key: Arc<str>,
    pinned_member: Option<i64>,
    reanchor: Option<ReanchorPlan>,
}

#[derive(Debug)]
struct ReanchorPlan {
    marker_key: Arc<str>,
    interval_secs: u64,
    anchored_member: Option<i64>,
}

pub(crate) fn take_session_id(headers: &mut HeaderMap) -> Option<String> {
    headers
        .remove(SESSION_HEADER)
        .and_then(|value| value.to_str().ok().map(str::trim).map(str::to_owned))
        .filter(|value| !value.is_empty())
}

fn member_key(
    route: &Route,
    user_key_id: Option<i64>,
    session_id: Option<&str>,
    conversation_fingerprint: Option<&[u8; 32]>,
) -> Option<Arc<str>> {
    if !enabled(route.settings_json.as_ref()) {
        return None;
    }
    let session_id = session_id.filter(|value| !value.is_empty());
    let subject = match (user_key_id, session_id) {
        (Some(user_id), Some(session_id)) => format!(
            "user:{user_id}:session:{}",
            blake3::hash(session_id.as_bytes()).to_hex()
        ),
        (Some(user_id), None)
            if conversation_subject(route.settings_json.as_ref())
                && conversation_fingerprint.is_some() =>
        {
            let fingerprint = conversation_fingerprint.expect("guarded above");
            format!(
                "user:{user_id}:conversation:v1:{}",
                blake3::Hash::from_bytes(*fingerprint).to_hex()
            )
        }
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

fn conversation_subject(settings: Option<&Value>) -> bool {
    settings
        .and_then(|settings| settings.get("affinity"))
        .and_then(|affinity| affinity.get("subject"))
        .and_then(Value::as_str)
        == Some("conversation")
}

fn reanchor_after_secs(settings: Option<&Value>) -> Option<u64> {
    settings
        .and_then(|settings| settings.get("affinity"))
        .and_then(|affinity| affinity.get("reanchor_after_secs"))
        .and_then(Value::as_u64)
        .filter(|secs| (1..=MAX_REANCHOR_SECS).contains(secs))
}

pub(super) async fn read_pin(cache: &dyn CacheBackend, key: Option<&str>) -> Option<i64> {
    let key = key?;
    cache.get(key).await.and_then(parse_pin)
}

/// Build the request-scoped plan and return the member pin, if it is valid for
/// the route's current hard re-anchor interval.
pub(super) async fn prepare(
    cache: &dyn CacheBackend,
    route: &Route,
    user_key_id: Option<i64>,
    session_id: Option<&str>,
    conversation_fingerprint: Option<&[u8; 32]>,
) -> Option<Arc<MemberAffinityPlan>> {
    let key = member_key(route, user_key_id, session_id, conversation_fingerprint)?;
    let reanchor = match reanchor_after_secs(route.settings_json.as_ref()) {
        Some(interval_secs) => {
            let marker_key: Arc<str> = format!("{key}{REANCHOR_SUFFIX}").into();
            // Remote cache backends make each get a network round trip.  The
            // two independent reads must stay concurrent.
            let (main_value, marker_value) =
                futures_util::join!(cache.get(&key), cache.get(&marker_key));
            let main_pin = main_value.and_then(parse_pin);
            let marker = marker_value.and_then(parse_marker);
            let anchored_member =
                main_pin.filter(|member_id| marker == Some((*member_id, interval_secs)));
            Some(ReanchorPlan {
                marker_key,
                interval_secs,
                anchored_member,
            })
        }
        None => {
            let pinned_member = read_pin(cache, Some(&key)).await;
            return Some(Arc::new(MemberAffinityPlan {
                key,
                pinned_member,
                reanchor: None,
            }));
        }
    };
    let pinned_member = reanchor.as_ref().and_then(|state| state.anchored_member);
    Some(Arc::new(MemberAffinityPlan {
        key,
        pinned_member,
        reanchor,
    }))
}

fn parse_pin(value: Vec<u8>) -> Option<i64> {
    String::from_utf8(value).ok()?.parse().ok()
}

fn parse_marker(value: Vec<u8>) -> Option<(i64, u64)> {
    let value = std::str::from_utf8(&value).ok()?;
    let mut parts = value.split(':');
    let version = parts.next()?;
    let member_id = parts.next()?.parse().ok()?;
    let interval_secs = parts.next()?.parse().ok()?;
    if version != "v1" || interval_secs == 0 || parts.next().is_some() {
        return None;
    }
    Some((member_id, interval_secs))
}

impl MemberAffinityPlan {
    pub(super) fn pinned_member(&self) -> Option<i64> {
        self.pinned_member
    }

    pub(super) async fn record_success(&self, cache: &dyn CacheBackend, member_id: i64) {
        // The main pin always slides, independently of the hard-age marker.
        write_pin(cache, &self.key, member_id).await;
        let Some(reanchor) = &self.reanchor else {
            return;
        };
        if reanchor.anchored_member == Some(member_id) {
            return;
        }
        let marker = format!("v1:{member_id}:{}", reanchor.interval_secs);
        let _ = cache
            .set(
                &reanchor.marker_key,
                marker.into_bytes(),
                Some(Duration::from_secs(reanchor.interval_secs)),
            )
            .await;
    }
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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::store::cache::{CacheError, CounterError, InvalidationHandler};

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Write {
        key: String,
        value: Vec<u8>,
        ttl: Option<Duration>,
    }

    #[derive(Default)]
    struct FakeCache {
        values: Mutex<HashMap<String, Vec<u8>>>,
        writes: Mutex<Vec<Write>>,
    }

    impl FakeCache {
        fn seed(&self, key: &str, value: &str) {
            self.values
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.as_bytes().to_vec());
        }

        fn clear_writes(&self) {
            self.writes.lock().unwrap().clear();
        }

        fn writes(&self) -> Vec<Write> {
            self.writes.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl CacheBackend for FakeCache {
        async fn get(&self, key: &str) -> Option<Vec<u8>> {
            self.values.lock().unwrap().get(key).cloned()
        }

        async fn set(
            &self,
            key: &str,
            value: Vec<u8>,
            ttl: Option<Duration>,
        ) -> Result<(), CacheError> {
            self.values
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.clone());
            self.writes.lock().unwrap().push(Write {
                key: key.to_owned(),
                value,
                ttl,
            });
            Ok(())
        }

        async fn incr(
            &self,
            _key: &str,
            _delta: i64,
            _ttl: Option<Duration>,
        ) -> Result<i64, CounterError> {
            Err(CounterError)
        }

        async fn delete(&self, key: &str) {
            self.values.lock().unwrap().remove(key);
        }

        async fn publish(&self, _channel: &str, _payload: &[u8]) {}

        async fn subscribe(&self, _channel: &str, _handler: InvalidationHandler) {}
    }

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

    fn affinity_route(extra: Value) -> Route {
        let mut affinity = json!({ "enabled": true });
        affinity
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        route(Some(json!({ "affinity": affinity })))
    }

    #[test]
    fn session_key_overrides_user_key() {
        let route = route(Some(json!({ "affinity": { "enabled": true } })));
        let user = member_key(&route, Some(9), None, None).expect("user affinity");
        let session = member_key(&route, Some(9), Some("chat-1"), None).expect("session affinity");
        assert_eq!(user.as_ref(), "route_aff:7:user:9");
        assert!(session.starts_with("route_aff:7:user:9:session:"));
        assert_ne!(session, user);
        assert_ne!(
            session,
            member_key(&route, Some(10), Some("chat-1"), None).expect("other user session")
        );
    }

    #[test]
    fn affinity_is_opt_in() {
        assert!(member_key(&route(None), Some(9), Some("chat-1"), None).is_none());
    }

    #[test]
    fn conversation_subject_is_header_then_fingerprint_then_user() {
        let route = affinity_route(json!({ "subject": "conversation" }));
        let fingerprint = [0x5a; 32];
        let digest = blake3::Hash::from_bytes(fingerprint).to_hex();
        let conversation = member_key(&route, Some(9), None, Some(&fingerprint)).unwrap();
        assert_eq!(
            conversation.as_ref(),
            format!("route_aff:7:user:9:conversation:v1:{digest}")
        );
        assert_eq!(
            member_key(&route, Some(9), None, None).unwrap().as_ref(),
            "route_aff:7:user:9"
        );
        let session = member_key(&route, Some(9), Some("explicit"), Some(&fingerprint)).unwrap();
        assert!(session.starts_with("route_aff:7:user:9:session:"));
        assert_ne!(session, conversation);

        let legacy = affinity_route(json!({}));
        assert_eq!(
            member_key(&legacy, Some(9), None, Some(&fingerprint))
                .unwrap()
                .as_ref(),
            "route_aff:7:user:9"
        );
    }

    #[test]
    fn invalid_reanchor_values_are_disabled() {
        for value in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("30"),
            json!(MAX_REANCHOR_SECS + 1),
            Value::Null,
        ] {
            let route = affinity_route(json!({ "reanchor_after_secs": value }));
            assert_eq!(reanchor_after_secs(route.settings_json.as_ref()), None);
        }
        let route = affinity_route(json!({ "reanchor_after_secs": MAX_REANCHOR_SECS }));
        assert_eq!(
            reanchor_after_secs(route.settings_json.as_ref()),
            Some(MAX_REANCHOR_SECS)
        );
    }

    #[tokio::test]
    async fn valid_same_member_slides_only_main_pin() {
        let cache = FakeCache::default();
        let route = affinity_route(json!({ "reanchor_after_secs": 60 }));
        let key = "route_aff:7:user:9";
        cache.seed(key, "11");
        cache.seed(&format!("{key}:reanchor"), "v1:11:60");

        let plan = prepare(&cache, &route, Some(9), None, None).await.unwrap();
        assert_eq!(plan.pinned_member(), Some(11));
        cache.clear_writes();
        plan.record_success(&cache, 11).await;

        assert_eq!(
            cache.writes(),
            vec![Write {
                key: key.into(),
                value: b"11".to_vec(),
                ttl: Some(AFFINITY_TTL),
            }]
        );

        cache.clear_writes();
        plan.record_success(&cache, 13).await;
        let writes = cache.writes();
        assert_eq!(writes[0].value.as_slice(), b"13");
        assert_eq!(writes[1].value.as_slice(), b"v1:13:60");
    }

    #[tokio::test]
    async fn mismatched_or_invalid_state_ignores_pin_and_reanchors() {
        for (main, marker) in [
            (Some("11"), None),
            (Some("11"), Some("broken")),
            (None, Some("v1:11:60")),
            (Some("11"), Some("v1:11:90")),
        ] {
            let cache = FakeCache::default();
            let route = affinity_route(json!({ "reanchor_after_secs": 60 }));
            let key = "route_aff:7:user:9";
            if let Some(main) = main {
                cache.seed(key, main);
            }
            if let Some(marker) = marker {
                cache.seed(&format!("{key}:reanchor"), marker);
            }

            let plan = prepare(&cache, &route, Some(9), None, None).await.unwrap();
            assert_eq!(plan.pinned_member(), None);
            cache.clear_writes();
            plan.record_success(&cache, 13).await;
            assert_eq!(
                cache.writes(),
                vec![
                    Write {
                        key: key.into(),
                        value: b"13".to_vec(),
                        ttl: Some(AFFINITY_TTL),
                    },
                    Write {
                        key: format!("{key}:reanchor"),
                        value: b"v1:13:60".to_vec(),
                        ttl: Some(Duration::from_secs(60)),
                    },
                ]
            );
        }
    }
}
