//! The ingress table is `gproxy_convert::surface`; this module adds its
//! OpenAPI projection.

pub use gproxy_convert::surface::{codex_path, is_surface, match_codex_path, match_path};

#[cfg(feature = "documentation")]
#[path = "surface_docs.rs"]
mod documentation;
#[cfg(feature = "documentation")]
pub(crate) use documentation::openapi;

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_protocol::Operation;
    use http::{HeaderMap, Method};

    #[test]
    fn codex_continuation_aliases_select_the_bound_call_on_the_canonical_path() {
        for path in [
            "/backend-api/codex/live/rtc_test",
            "/api/codex/live/rtc_test",
            "/codex/live/rtc_test",
        ] {
            let matched = match_codex_path(&Method::GET, path, &HeaderMap::new(), None).unwrap();
            assert_eq!(matched.operation.operation, Operation::ConnectRealtime);
            assert_eq!(
                gproxy_core::realtime::connect::model(
                    matched.operation,
                    &codex_path(path),
                    None,
                    None
                )
                .unwrap(),
                None
            );
        }
    }
}
