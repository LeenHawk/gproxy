//! The ingress table: which HTTP method and path mean which
//! [`OperationKey`].
//!
//! `gproxy-protocol` deliberately has no paths — "which URL serves an
//! operation is an HTTP convention owned by the ingress layer", and this crate
//! is that layer. The table below is the inverse of
//! `gproxy-core::convert::endpoints`, which answers the same question in the
//! other direction for a converted request, and a test checks that every key
//! named here is one `OPERATION_SPECS` declares.
//!
//! # Two things the path alone does not say
//!
//! **Streaming.** `/v1/messages` is `GenerateContent`, and the same path with
//! `{"stream":true}` is `StreamGenerateContent`. They are separate operations
//! — separate routing rules, separate settlement — so the flag is read here
//! rather than left for a channel to notice. Gemini says it in the path
//! instead (`:streamGenerateContent`), which is why that dialect has two rows.
//!
//! **Which vendor.** `/v1/models` and `/v1/files` are spelled identically by
//! OpenAI, Claude and Gemini's v1 surface. The dialect decides which wire
//! types a transform is asked for, so guessing it wrong is a converted body
//! the client cannot parse. The tiebreak is the client's own authentication
//! header — `x-goog-api-key` is Gemini's, `anthropic-version` is Claude's —
//! which is evidence the client supplied about itself rather than a default
//! this crate invented. With no evidence the first row wins, and the table is
//! ordered so that is OpenAI.

use gproxy_protocol::{Dialect, Operation, OperationKey};
use http::{HeaderMap, Method};
use serde_json::Value;

/// One declared ingress path.
struct Surface {
    method: Method,
    /// Literal segments, `{name}` for one segment, and `{model}:action` for
    /// Gemini's method suffix.
    pattern: &'static str,
    operation: Operation,
    /// The operation the same path means when the body asks to stream. `None`
    /// for the paths where streaming is not a body flag.
    streaming: Option<Operation>,
    dialect: Dialect,
    /// The path parameter that carries the model name, when it does.
    model_param: Option<&'static str>,
    upgrade: bool,
}

/// What a matched path resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matched {
    pub operation: OperationKey,
    /// The model the path named, for the dialects that put it there.
    pub model: Option<String>,
    /// This surface is a websocket handshake.
    pub upgrade: bool,
}

macro_rules! surfaces {
    ($table:ident; $(
        $method:ident $pattern:literal => $operation:ident / $dialect:ident
        $(, stream: $streaming:ident)?
        $(, model: $model:literal)?
        $(, upgrade: $upgrade:literal)?
    ;)*) => {
        fn $table() -> &'static [Surface] {
            static TABLE: std::sync::OnceLock<Vec<Surface>> = std::sync::OnceLock::new();
            TABLE.get_or_init(|| vec![$(
                Surface {
                    method: Method::$method,
                    pattern: $pattern,
                    operation: Operation::$operation,
                    streaming: None $(.or(Some(Operation::$streaming)))?,
                    dialect: Dialect::$dialect,
                    model_param: None $(.or(Some($model)))?,
                    upgrade: false $(|| $upgrade)?,
                },
            )*])
        }
    };
}

surfaces! { table;
    // -------------------------------------------------- content generation --
    POST "/v1/responses" => GenerateContent / OpenAi, stream: StreamGenerateContent;
    POST "/v1/responses/compact" => CompactContent / OpenAi;
    POST "/v1/chat/completions" => GenerateContent / OpenAiChat, stream: StreamGenerateContent;
    POST "/v1/messages" => GenerateContent / Claude, stream: StreamGenerateContent;
    POST "/v1/messages/count_tokens" => CountTokens / Claude;
    POST "/v1/responses/input_tokens" => CountTokens / OpenAi;
    POST "/v1beta/models/{model}:generateContent" => GenerateContent / Gemini, model: "model";
    POST "/v1beta/models/{model}:streamGenerateContent" => StreamGenerateContent / Gemini, model: "model";
    POST "/v1beta/models/{model}:countTokens" => CountTokens / Gemini, model: "model";
    POST "/v1beta/models/{model}:embedContent" => CreateEmbedding / Gemini, model: "model";
    POST "/v1beta/models/{model}:batchEmbedContents" => BatchCreateEmbedding / Gemini, model: "model";
    // ------------------------------------------------------------- models --
    // The OpenAI rows come first because they are the default when a client
    // supplied no evidence about which vendor it is.
    GET "/v1/models" => ListModels / OpenAi;
    GET "/v1/models/{id}" => GetModel / OpenAi, model: "id";
    GET "/v1/models" => ListModels / Claude;
    GET "/v1/models/{id}" => GetModel / Claude, model: "id";
    GET "/v1beta/models" => ListModels / Gemini;
    GET "/v1beta/models/{id}" => GetModel / Gemini, model: "id";
    // --------------------------------------------------------- everything --
    POST "/v1/embeddings" => CreateEmbedding / OpenAi;
    POST "/v1/moderations" => CreateModeration / OpenAi;
    POST "/v1/decisions" => CreateDecision / OpenAi;
    POST "/v1/rerank" => Rerank / OpenAi;
    POST "/v1/conversations" => CreateConversation / OpenAi;
    POST "/v1/search" => WebSearch / OpenAi;
    POST "/v1/alpha/search" => WebSearch / OpenAi;
    POST "/alpha/search" => WebSearch / OpenAi;
    POST "/v1/images/generations" => CreateImage / OpenAi;
    POST "/v1/images/edits" => EditImage / OpenAi;
    POST "/v1/audio/speech" => CreateSpeech / OpenAi;
    POST "/v1/audio/transcriptions" => CreateTranscription / OpenAi;
    POST "/v1/audio/translations" => CreateTranslation / OpenAi;
    // -------------------------------------------------------------- files --
    GET "/v1/files" => ListFiles / OpenAi;
    POST "/v1/files" => CreateFile / OpenAi;
    GET "/v1/files/{id}" => RetrieveFile / OpenAi;
    DELETE "/v1/files/{id}" => DeleteFile / OpenAi;
    GET "/v1/files/{id}/content" => RetrieveFileContent / OpenAi;
    // Claude spells the file API at the same paths; the tiebreak is the
    // client's own `anthropic-version` or `x-api-key`.
    GET "/v1/files" => ListFiles / Claude;
    POST "/v1/files" => CreateFile / Claude;
    GET "/v1/files/{id}" => RetrieveFile / Claude;
    DELETE "/v1/files/{id}" => DeleteFile / Claude;
    GET "/v1/files/{id}/content" => RetrieveFileContent / Claude;
    GET "/v1beta/files" => ListFiles / Gemini;
    POST "/v1beta/files" => CreateFile / Gemini;
    POST "/upload/v1beta/files" => CreateFile / Gemini;
    GET "/v1beta/files/{id}" => RetrieveFile / Gemini;
    DELETE "/v1beta/files/{id}" => DeleteFile / Gemini;
    GET "/v1beta/files/{id}:download" => RetrieveFileContent / Gemini;
    // ------------------------------------------------------------- videos --
    GET "/v1/videos" => ListVideos / OpenAi;
    POST "/v1/videos" => CreateVideo / OpenAi;
    GET "/v1/videos/{id}" => RetrieveVideo / OpenAi;
    DELETE "/v1/videos/{id}" => DeleteVideo / OpenAi;
    GET "/v1/videos/{id}/content" => DownloadVideoContent / OpenAi;
    // ----------------------------------------------------------- realtime --
    // `POST /v1/realtime/calls` is the SDP offer, and it is HTTP: the
    // handshake that continues it carries no body at all (`WireRequest<()>`
    // all the way down to `Upstream::connect`). What survives the upgrade is
    // the *call id*, in the path or in `?call_id=`, which core looks up to pin
    // the socket to the credential the offer was answered by.
    //
    // The three upgrade paths are the three `RealtimeRoute`s that
    // `Core::connect_realtime_path` accepts, so this table is that guard: a
    // path it does not declare never reaches the engine, and a test checks the
    // two agree.
    POST "/v1/realtime/calls" => CreateRealtimeCall / OpenAi;
    POST "/v1/live" => CreateRealtimeCall / OpenAi;
    GET "/v1/realtime" => ConnectRealtime / OpenAi, upgrade: true;
    GET "/v1/live" => ConnectRealtime / OpenAi, upgrade: true;
    GET "/v1/live/{call_id}" => ConnectRealtime / OpenAi, upgrade: true;
    GET "/v1/responses" => GenerateContent / OpenAiResponsesWebSocket, upgrade: true;
    GET "/ws/v1beta/BidiGenerateContent" => ConnectRealtime / Gemini, upgrade: true;
}

// These are Codex wire contracts, available only on a Codex provider mount.
// Standard OpenAI operations remain in `table`, including on that mount.
surfaces! { codex_table;
    POST "/v1/memories/trace_summarize" => SummarizeMemory / OpenAi;
    POST "/v1/guardian" => GuardianReview / OpenAi;
    POST "/v1/guardian-classifier" => GuardianClassify / OpenAi;
}

/// Codex's native backend URLs and our scoped `/v1` spelling resolve to the
/// same operation. This is called only after identifying a Codex provider.
pub fn match_codex_path(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: Option<&Value>,
) -> Option<Matched> {
    let path = codex_path(path);
    match_surfaces(codex_table(), method, &path, headers, body)
        .or_else(|| match_path(method, &path, headers, body))
}

pub fn codex_path(path: &str) -> std::borrow::Cow<'_, str> {
    ["/backend-api/codex/", "/api/codex/", "/codex/"]
        .into_iter()
        .find_map(|prefix| {
            path.strip_prefix(prefix)
                .map(|rest| format!("/v1/{rest}").into())
        })
        .unwrap_or(std::borrow::Cow::Borrowed(path))
}

/// Whether `path` is a surface this gateway serves at all, for the mount
/// grammar. Method-sensitive, because `/v1/files` is a surface for `GET` and
/// `POST` and nothing else.
pub fn is_surface(method: &Method, path: &str) -> bool {
    table()
        .iter()
        .any(|surface| surface.method == method && match_pattern(surface.pattern, path).is_some())
}

/// The operation `method` and `path` name, with the model the path carried.
///
/// `headers` and `body` resolve the two things a path cannot say: which vendor
/// spelled an ambiguous path, and whether the caller asked to stream.
pub fn match_path(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: Option<&Value>,
) -> Option<Matched> {
    match_surfaces(table(), method, path, headers, body)
}

fn match_surfaces(
    surfaces: &[Surface],
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: Option<&Value>,
) -> Option<Matched> {
    let preferred = preferred_dialect(headers);
    let mut fallback: Option<&Surface> = None;
    let mut chosen: Option<&Surface> = None;
    let mut captured = Vec::new();
    for surface in surfaces {
        if surface.method != method {
            continue;
        }
        let Some(params) = match_pattern(surface.pattern, path) else {
            continue;
        };
        if preferred.is_some_and(|dialect| dialect.family() == surface.dialect.family()) {
            chosen = Some(surface);
            captured = params;
            break;
        }
        if fallback.is_none() {
            fallback = Some(surface);
            captured = params;
        }
    }
    let surface = chosen.or(fallback)?;
    let operation = match surface.streaming {
        Some(streaming) if wants_stream(body) => streaming,
        _ => surface.operation,
    };
    let model = surface.model_param.and_then(|name| {
        captured
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    });
    Some(Matched {
        operation: OperationKey {
            operation,
            dialect: surface.dialect,
        },
        model,
        upgrade: surface.upgrade,
    })
}

/// The vendor the client's own headers identify it as.
///
/// Only the authentication headers, which a client sends because its SDK
/// always does. `anthropic-version` is included because the Claude SDK sends
/// it on every request and a bearer token alone would be ambiguous.
fn preferred_dialect(headers: &HeaderMap) -> Option<Dialect> {
    if headers.contains_key("x-goog-api-key") {
        return Some(Dialect::Gemini);
    }
    if headers.contains_key("anthropic-version") || headers.contains_key("x-api-key") {
        return Some(Dialect::Claude);
    }
    None
}

/// The `stream` flag every JSON dialect that has one spells the same way.
fn wants_stream(body: Option<&Value>) -> bool {
    body.and_then(|body| body.get("stream")?.as_bool())
        .unwrap_or(false)
}

/// Match a path against one pattern.
///
/// Three segment forms, which is all the declared surfaces need: a literal,
/// `{name}` for exactly one non-empty segment, and `{name}:action` for
/// Gemini's method suffix. No trailing-rest form: every surface here names its
/// whole path, and a `{*rest}` would silently swallow a path nobody declared.
fn match_pattern(pattern: &'static str, path: &str) -> Option<Vec<(&'static str, String)>> {
    let mut captured = Vec::new();
    let mut segments = path.strip_prefix('/')?.split('/');
    for expected in pattern.strip_prefix('/')?.split('/') {
        let segment = segments.next()?;
        if segment.is_empty() {
            return None;
        }
        match parameter(expected) {
            Some((name, None)) => captured.push((name, decode(segment))),
            Some((name, Some(action))) => {
                let value = segment.strip_suffix(action)?.strip_suffix(':')?;
                if value.is_empty() {
                    return None;
                }
                captured.push((name, decode(value)));
            }
            None if expected == segment => {}
            None => return None,
        }
    }
    segments.next().is_none().then_some(captured)
}

/// `{name}` or `{name}:action`, as the name and the action.
fn parameter(segment: &'static str) -> Option<(&'static str, Option<&'static str>)> {
    let inner = segment.strip_prefix('{')?;
    match inner.split_once('}') {
        Some((name, "")) => Some((name, None)),
        Some((name, rest)) => Some((name, Some(rest.strip_prefix(':')?))),
        None => None,
    }
}

/// A path segment's percent-encoding undone. A model name carries `/`, `:`
/// and `.`, and the dialects encode it before putting it in a path, so the
/// name handed to resolution has to be the decoded one.
fn decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                char::from(bytes[index + 1]).to_digit(16),
                char::from(bytes[index + 2]).to_digit(16),
            )
        {
            out.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| segment.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_protocol::spec::OPERATION_SPECS;
    use serde_json::json;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn every_declared_surface_names_a_real_operation_key() {
        // The whole point of keying on `OperationKey` is that a path table
        // cannot invent a combination the protocol crate does not model.
        for surface in table() {
            for operation in [Some(surface.operation), surface.streaming] {
                let Some(operation) = operation else { continue };
                let key = OperationKey {
                    operation,
                    dialect: surface.dialect,
                };
                assert!(
                    OPERATION_SPECS.iter().any(|spec| spec.key == key),
                    "{} {} declares {key:?}, which OPERATION_SPECS does not",
                    surface.method,
                    surface.pattern
                );
            }
        }
    }

    #[test]
    fn no_two_surfaces_of_one_dialect_share_a_method_and_path() {
        for (index, surface) in table().iter().enumerate() {
            for other in &table()[index + 1..] {
                assert!(
                    !(surface.method == other.method
                        && surface.pattern == other.pattern
                        && surface.dialect == other.dialect),
                    "{} {} is declared twice",
                    surface.method,
                    surface.pattern
                );
            }
        }
    }

    #[test]
    fn the_generation_paths_resolve_to_their_dialects() {
        let matched = match_path(&Method::POST, "/v1/messages", &HeaderMap::new(), None).unwrap();
        assert_eq!(
            matched.operation,
            OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Claude
            }
        );
        assert_eq!(matched.model, None);

        let matched = match_path(
            &Method::POST,
            "/v1/chat/completions",
            &HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(matched.operation.dialect, Dialect::OpenAiChat);
        let matched = match_path(&Method::POST, "/v1/responses", &HeaderMap::new(), None).unwrap();
        assert_eq!(matched.operation.dialect, Dialect::OpenAi);
    }

    #[test]
    fn codex_standalone_search_paths_reach_the_data_plane() {
        for path in ["/v1/search", "/v1/alpha/search", "/alpha/search"] {
            let matched = match_path(&Method::POST, path, &HeaderMap::new(), None).unwrap();
            assert_eq!(
                matched.operation,
                OperationKey {
                    operation: Operation::WebSearch,
                    dialect: Dialect::OpenAi
                }
            );
        }
    }

    #[test]
    fn a_stream_flag_selects_the_streaming_operation() {
        let body = json!({"model": "m", "stream": true});
        let matched = match_path(
            &Method::POST,
            "/v1/messages",
            &HeaderMap::new(),
            Some(&body),
        )
        .unwrap();
        assert_eq!(
            matched.operation.operation,
            Operation::StreamGenerateContent
        );
        let body = json!({"model": "m", "stream": false});
        let matched = match_path(
            &Method::POST,
            "/v1/messages",
            &HeaderMap::new(),
            Some(&body),
        )
        .unwrap();
        assert_eq!(matched.operation.operation, Operation::GenerateContent);
        // A non-boolean flag is not a request to stream.
        let body = json!({"stream": "yes"});
        let matched = match_path(
            &Method::POST,
            "/v1/messages",
            &HeaderMap::new(),
            Some(&body),
        )
        .unwrap();
        assert_eq!(matched.operation.operation, Operation::GenerateContent);
    }

    #[test]
    fn gemini_names_its_model_and_its_streaming_in_the_path() {
        let matched = match_path(
            &Method::POST,
            "/v1beta/models/gemini-2.5-pro:generateContent",
            &HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(matched.operation.dialect, Dialect::Gemini);
        assert_eq!(matched.operation.operation, Operation::GenerateContent);
        assert_eq!(matched.model.as_deref(), Some("gemini-2.5-pro"));

        let matched = match_path(
            &Method::POST,
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
            &HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(
            matched.operation.operation,
            Operation::StreamGenerateContent
        );

        // A percent-encoded name comes back decoded, because that is the name
        // resolution has to match against.
        let matched = match_path(
            &Method::POST,
            "/v1beta/models/acme%2Ffast:generateContent",
            &HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(matched.model.as_deref(), Some("acme/fast"));
    }

    #[test]
    fn an_ambiguous_path_is_decided_by_the_clients_own_header() {
        let openai = match_path(&Method::GET, "/v1/models", &HeaderMap::new(), None).unwrap();
        assert_eq!(openai.operation.dialect, Dialect::OpenAi);

        let claude = match_path(
            &Method::GET,
            "/v1/models",
            &headers(&[("anthropic-version", "2023-06-01")]),
            None,
        )
        .unwrap();
        assert_eq!(claude.operation.dialect, Dialect::Claude);

        // Gemini's own path is unambiguous, and its header does not move an
        // OpenAI-only path onto a dialect that has no row for it.
        let gemini = match_path(
            &Method::GET,
            "/v1beta/models",
            &headers(&[("x-goog-api-key", "k")]),
            None,
        )
        .unwrap();
        assert_eq!(gemini.operation.dialect, Dialect::Gemini);
        let responses = match_path(
            &Method::POST,
            "/v1/responses",
            &headers(&[("x-goog-api-key", "k")]),
            None,
        )
        .unwrap();
        assert_eq!(responses.operation.dialect, Dialect::OpenAi);
    }

    #[test]
    fn a_path_that_is_not_declared_matches_nothing() {
        for (method, path) in [
            (Method::GET, "/v1/messages"),
            (Method::POST, "/v1/models"),
            (Method::POST, "/v1/messages/"),
            (Method::POST, "/v1/messages/extra"),
            (Method::POST, "/v1beta/models/:generateContent"),
            (Method::POST, "/v1beta/models/m:unknownAction"),
            (Method::POST, "/backend-api/codex/responses"),
            (Method::GET, "/"),
        ] {
            assert!(
                match_path(&method, path, &HeaderMap::new(), None).is_none(),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn is_surface_is_what_the_mount_grammar_asks() {
        assert!(is_surface(&Method::POST, "/v1/messages"));
        assert!(is_surface(&Method::GET, "/v1/models"));
        assert!(!is_surface(&Method::GET, "/v1/messages"));
        assert!(!is_surface(&Method::GET, "/backend-api/wham/usage"));
    }

    #[test]
    fn the_websocket_surfaces_are_declared_and_marked() {
        let matched = match_path(&Method::GET, "/v1/realtime", &HeaderMap::new(), None).unwrap();
        assert!(matched.upgrade);
        let matched = match_path(&Method::POST, "/v1/messages", &HeaderMap::new(), None).unwrap();
        assert!(!matched.upgrade);
    }

    #[test]
    fn every_upgrade_surface_names_an_operation_the_protocol_calls_a_websocket() {
        use gproxy_protocol::spec::OperationTransport;

        // The flag and the transport must not be able to disagree: a surface
        // marked `upgrade` that core would execute over HTTP would answer a
        // `101` and then hand the pump a socket that does not exist.
        for surface in table() {
            let key = OperationKey {
                operation: surface.operation,
                dialect: surface.dialect,
            };
            let websocket = OPERATION_SPECS
                .iter()
                .find(|spec| spec.key == key)
                .is_some_and(|spec| matches!(spec.transport, OperationTransport::WebSocket));
            assert_eq!(
                surface.upgrade, websocket,
                "{} {} is marked upgrade: {}",
                surface.method, surface.pattern, surface.upgrade
            );
        }
    }

    #[test]
    fn the_openai_realtime_upgrades_are_exactly_the_routes_core_dispatches() {
        use gproxy_protocol::wire::openai::realtime::RealtimeRoute;

        // `Core::connect_realtime_path` accepts `Connect` and `Live`; this
        // table is the host's copy of that grammar, so each declared path has
        // to be one of them and each of them has to be declared.
        let declared: Vec<&str> = table()
            .iter()
            .filter(|surface| {
                surface.upgrade
                    && surface.operation == Operation::ConnectRealtime
                    && surface.dialect == Dialect::OpenAi
            })
            .map(|surface| surface.pattern)
            .collect();
        assert_eq!(declared, ["/v1/realtime", "/v1/live", "/v1/live/{call_id}"]);
        for (pattern, path) in [
            ("/v1/realtime", "/v1/realtime"),
            ("/v1/live", "/v1/live"),
            ("/v1/live/{call_id}", "/v1/live/rtc_abc"),
        ] {
            assert!(
                declared.contains(&pattern),
                "{pattern} is no longer declared"
            );
            assert!(
                matches!(
                    RealtimeRoute::from_path(path),
                    Some(RealtimeRoute::Connect | RealtimeRoute::Live { .. })
                ),
                "{path} is not a route core would dispatch"
            );
            let matched = match_path(&Method::GET, path, &HeaderMap::new(), None).unwrap();
            assert!(matched.upgrade);
            assert_eq!(matched.operation.operation, Operation::ConnectRealtime);
            // The call id is a path segment, not a model: naming it `model`
            // would send `rtc_abc` to the resolver.
            assert_eq!(matched.model, None);
        }
        // The SDP offer keeps its own HTTP row and is not an upgrade.
        let offer =
            match_path(&Method::POST, "/v1/realtime/calls", &HeaderMap::new(), None).unwrap();
        assert!(!offer.upgrade);
        assert_eq!(offer.operation.operation, Operation::CreateRealtimeCall);
    }
}
