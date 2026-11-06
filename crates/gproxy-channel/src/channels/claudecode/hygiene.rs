//! The request hygiene the CLI's own requests satisfy, applied to client
//! Messages bodies so the OAuth surface accepts them: content canonicalized
//! to block arrays, empty text blocks removed with their `cache_control`
//! re-attached to the nearest cacheable block, thinking blocks kept only on
//! assistant turns, all sampling parameters removed for API 2026-10-21,
//! unsupported assistant prefills removed, feature betas derived from the body,
//! and the CLI's billing system block plus `metadata.user_id`
//! (v3 `shared/claude/{cache,hygiene}.rs` and `claudecode/cch.rs`). The
//! canonicalize/empty-block pass lives in `channels::shared::cache`, where the
//! magic cache strings that build on it live.

use crate::channels::shared::cache;
use http::{HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const THINKING_DISPLAY_UPDATES_BETA: &str = "thinking-display-updates-2026-08-18";
/// Salt of the CLI's `cc_version` build suffix (v3 `cch.rs`).
const SUFFIX_SALT: &str = "59cf53e54c78";

/// Models that still accept an assistant prefill.
const PREFILL_TOLERANT: &[&str] = &[
    "claude-3-opus",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
];

pub(super) fn json_object(body: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
}

/// A body carrying a server-side fallback credit is replayed verbatim.
fn has_fallback_credit(body: &Value) -> bool {
    body.get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
}

/// `magic_cache` places `cache_control` at the magic cache strings; without
/// it the strings are only stripped.
pub(super) fn messages(body: &mut Value, headers: &mut HeaderMap, magic_cache: bool) {
    if has_fallback_credit(body) {
        return;
    }
    if magic_cache {
        cache::apply(body, cache::Rules::Claude);
    } else {
        // Strip first so a text block that only trailed a token trims the
        // same way a token-free one does.
        cache::strip_tokens(body);
        cache::sanitize_claude(body);
    }
    strip_sampling(body);
    coerce_prefill(body);
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
    strip_beta(headers, CONTEXT_1M_BETA);
}

pub(super) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
}

// --------------------------------------------------------------- billing

/// How long a session's last upstream `request-id` is remembered for
/// `cc_prev_req`.
pub(super) const PREV_REQ_TTL_MS: i64 = 60 * 60 * 1000;

const BILLING_PREFIX: &str = "x-anthropic-billing-header:";

/// The CLI's `x-anthropic-billing-header` system block and the
/// `metadata.user_id` JSON the backend correlates it with (v3 `cch.rs`;
/// captured in `samples/claude-code-2.1.252/messages-oauth-wire.json`).
///
/// The block is rebuilt in the CLI's own order (2.1.284):
/// `cc_version` (ours, with the computed suffix), `cc_entrypoint` (the
/// client's, default `cli`), `cch=00000`, then the optional fragments the
/// client sent when they pass the CLI's own validation: `cc_workload`,
/// `cc_is_subagent=true`, `cc_prev_req`, `cc_prompt_id`, `cc_turn_origin`,
/// then the paired `cc_prompt_index` / `cc_turn_index`. CLI 2.1.316 adds the
/// paired `cc_project` / `cc_project_src` for a Claude project, and appends
/// `cc_prev_msg` and paired `cc_prev_prompt_index` / `cc_prev_turn_index`
/// after compaction; preserve validated client values without inferring them
/// from the compacted transcript. Unknown keys and
/// invalid values are dropped; they would only fingerprint the proxy. Gateway
/// attribution (`cc_gateway`) is also dropped: this channel impersonates the
/// default direct OAuth client, not a gateway-configured CLI. Two
/// fragments the CLI's main loop sends on the OAuth path are synthesized when the
/// client left them out: `cc_prev_req` from `prev_req`, the `request-id` of
/// the session's previous upstream call, and `cc_prompt_id`, a UUID derived
/// from the device, the session and the count of user text turns, so it
/// stays fixed across the tool loop of one prompt and changes with the next
/// user message; this approximates the CLI's per-prompt id. Both are only
/// synthesized for a main-loop turn: a client block that carries none of the
/// turn fragments is a CLI side call and stays bare. `cc_workload`
/// and `cc_is_subagent` stay absent unless sent: absence is the main session.
///
/// The turn fields of a client that sent no billing block are synthesized
/// as a main-loop turn (see below).
///
/// `sdk_entrypoint` is set when the channel presents as the Agent SDK. It
/// then replaces the client's `cc_entrypoint`, and a main-loop turn is an
/// SDK one (`cc_turn_origin=sdk`, `cc_prompt_index=0`, `cc_turn_index` the
/// client's or the count of user text turns, as in the `-p` captures).
///
/// `identity_block` places the identity system block after the billing
/// block, matching the entrypoint the billing block carries.
pub(super) fn inject_billing(
    body: &mut Value,
    device_id: &str,
    account_uuid: &str,
    session: &str,
    prev_req: Option<&str>,
    sdk_entrypoint: Option<&str>,
    identity_block: bool,
) {
    if has_fallback_credit(body) {
        return;
    }
    let suffix = version_suffix(first_user_text(body));
    let prompt_index = user_text_turns(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let user_id = json!({
        "device_id": device_id,
        "account_uuid": account_uuid,
        "session_id": session,
    })
    .to_string();
    let metadata = root
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("user_id".into(), Value::String(user_id));
    }

    let system = root
        .entry("system")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !system.is_array() {
        let previous = std::mem::take(system);
        *system = Value::Array(vec![previous]);
    }
    let Some(blocks) = system.as_array_mut() else {
        return;
    };
    let existing = blocks.iter().position(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with(BILLING_PREFIX))
    });
    let sent = existing
        .and_then(|index| blocks[index].get("text"))
        .and_then(Value::as_str)
        .map(billing_fields)
        .unwrap_or_default();
    let field = |name: &str| {
        sent.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    };

    let entrypoint = sdk_entrypoint.unwrap_or_else(|| {
        field("cc_entrypoint")
            .filter(|value| is_token(value))
            .unwrap_or("cli")
    });
    let sdk = entrypoint.starts_with("sdk-");
    let mut text = format!(
        "{BILLING_PREFIX} cc_version={}.{suffix}; cc_entrypoint={entrypoint}; cch=00000;",
        super::CLI_VERSION,
    );
    if let Some(workload) = field("cc_workload").filter(|value| is_token(value)) {
        text.push_str(&format!(" cc_workload={workload};"));
    }
    if field("cc_is_subagent") == Some("true") {
        text.push_str(" cc_is_subagent=true;");
    }
    // The CLI's main loop sends the turn fragments (2.1.316 `JXs`); its
    // side calls (`QXs`, e.g. session titles) send none of them. A client
    // billing block without any is such a side call and stays without them.
    // A client that sent no block is a gateway client whose every request is
    // a main-loop turn.
    let sent_prev_req = field("cc_prev_req").filter(|value| is_request_id(value));
    let sent_prompt = field("cc_prompt_id").filter(|value| is_uuid(value));
    let sent_origin = field("cc_turn_origin").filter(|value| is_turn_origin(value));
    let turn = field("cc_turn_index").and_then(|value| billing_index(value, 1));
    let sent_indices = field("cc_prompt_index")
        .and_then(|value| billing_index(value, 0))
        .zip(turn);
    let gateway_client = existing.is_none();
    let main_turn = gateway_client
        || sent_prev_req.is_some()
        || sent_prompt.is_some()
        || sent_origin.is_some()
        || sent_indices.is_some();
    if let Some(request) =
        sent_prev_req.or_else(|| prev_req.filter(|value| main_turn && is_request_id(value)))
    {
        text.push_str(&format!(" cc_prev_req={request};"));
    }
    let prompt = match sent_prompt {
        Some(prompt) => Some(prompt.to_owned()),
        None => main_turn.then(|| {
            super::derived_uuid(&format!(
                "claudecode-prompt:{device_id}:{session}:{prompt_index}"
            ))
        }),
    };
    if let Some(prompt) = prompt {
        text.push_str(&format!(" cc_prompt_id={prompt};"));
    }
    // A gateway client's turn is one per user text turn: `human` for the
    // CLI and `sdk` (prompt index 0, since SDK input is never human) for the
    // Agent SDK, as in the captures. Counters are client session state:
    // prompt advances only for human input, turn for every new triggering
    // input (not tool round-trips).
    let turns = u32::try_from(prompt_index.max(1)).unwrap_or(u32::MAX);
    let (origin, indices) = match sdk_entrypoint {
        Some(_) if main_turn => (Some("sdk"), Some((0, turn.unwrap_or(turns)))),
        Some(_) => (None, None),
        None if gateway_client => (Some("human"), Some((turns, turns))),
        None => (sent_origin, sent_indices),
    };
    if let Some(origin) = origin {
        text.push_str(&format!(" cc_turn_origin={origin};"));
    }
    // CLI 2.1.284 emits both indices or neither.
    if let Some((prompt, turn)) = indices
        && prompt <= turn
    {
        text.push_str(&format!(" cc_prompt_index={prompt}; cc_turn_index={turn};"));
    }
    // 2.1.316 attributes a request to a Claude project; the id and its
    // settings source travel together.
    if let Some(project) = field("cc_project").filter(|value| is_project_id(value))
        && let Some(source) =
            field("cc_project_src").filter(|value| PROJECT_SOURCES.contains(value))
    {
        text.push_str(&format!(" cc_project={project}; cc_project_src={source};"));
    }
    if let Some(message) = field("cc_prev_msg").filter(|value| is_message_id(value)) {
        text.push_str(&format!(" cc_prev_msg={message};"));
    }
    if let Some(prompt) = field("cc_prev_prompt_index").and_then(|value| billing_index(value, 0))
        && let Some(turn) = field("cc_prev_turn_index").and_then(|value| billing_index(value, 1))
    {
        text.push_str(&format!(
            " cc_prev_prompt_index={prompt}; cc_prev_turn_index={turn};"
        ));
    }

    let billing = json!({"type": "text", "text": text});
    let billing_at = match existing {
        Some(index) => {
            blocks[index] = billing;
            index
        }
        None => {
            blocks.insert(0, billing);
            0
        }
    };
    if identity_block {
        identity(blocks, billing_at, sdk);
    }
}

/// The CLI's identity strings (2.1.316 `oe`, `Ue`, `Ie`).
const CLI_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";
const SDK_APPEND_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude, running within the Claude Agent SDK.";
const SDK_IDENTITY: &str = "You are a Claude agent, built on Anthropic's Claude Agent SDK.";

/// The identity block follows the billing block (`p7n`). An interactive
/// entrypoint gets `CLI_IDENTITY`; a non-interactive one (`sdk-*`, `-p`
/// included) keeps a client's SDK identity and otherwise gets
/// `SDK_IDENTITY`, the pick for a custom system prompt, which a gateway
/// client's is. A client's identity of the other family is rewritten; one
/// is inserted when the client sent none.
fn identity(blocks: &mut Vec<Value>, billing_at: usize, sdk: bool) {
    let wanted = if sdk { SDK_IDENTITY } else { CLI_IDENTITY };
    let identity = blocks.iter().position(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| [CLI_IDENTITY, SDK_APPEND_IDENTITY, SDK_IDENTITY].contains(&text))
    });
    match identity {
        Some(index) => {
            let current = blocks[index].get("text").and_then(Value::as_str);
            let matches = if sdk {
                current != Some(CLI_IDENTITY)
            } else {
                current == Some(CLI_IDENTITY)
            };
            if !matches && let Some(block) = blocks[index].as_object_mut() {
                block.insert("text".into(), Value::String(wanted.into()));
            }
        }
        None => blocks.insert(billing_at + 1, json!({"type": "text", "text": wanted})),
    }
}

/// The `key=value;` fragments of a billing block, in order.
fn billing_fields(text: &str) -> Vec<(&str, &str)> {
    text.strip_prefix(BILLING_PREFIX)
        .unwrap_or(text)
        .split(';')
        .filter_map(|field| field.trim().split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .filter(|(key, value)| !key.is_empty() && !value.is_empty())
        .collect()
}

/// The CLI serializes integer counters in decimal, bounded by 10 million.
fn billing_index(value: &str, minimum: u32) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|value| (minimum..=10_000_000).contains(value))
}

/// `[A-Za-z0-9_-]+`, the CLI's charset for entrypoints and workloads.
fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// CLI 2.1.280: `^[a-z][a-z_]{0,31}$`.
fn is_turn_origin(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

/// The settings sources a project attribution may come from (2.1.316 `M`).
const PROJECT_SOURCES: &[&str] = &[
    "harness",
    "user",
    "project",
    "local",
    "flag",
    "managed",
    "admin_default",
];

/// `^(?:claude_proj|chan)_(?:staging_|local_)?01[1-9A-HJ-NP-Za-km-z]{22}$`
/// (2.1.318 `DSs`; 2.1.316 accepted only `claude_proj_`).
fn is_project_id(value: &str) -> bool {
    let Some(rest) = value
        .strip_prefix("claude_proj_")
        .or_else(|| value.strip_prefix("chan_"))
    else {
        return false;
    };
    let rest = rest
        .strip_prefix("staging_")
        .or_else(|| rest.strip_prefix("local_"))
        .unwrap_or(rest);
    rest.strip_prefix("01").is_some_and(|id| {
        id.len() == 22
            && id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() && !matches!(byte, b'0' | b'I' | b'O' | b'l')
            })
    })
}

/// `^req_[A-Za-z0-9_-]{1,36}$` (2.1.252 `KUt`).
pub(super) fn is_request_id(value: &str) -> bool {
    value
        .strip_prefix("req_")
        .is_some_and(|rest| (1..=36).contains(&rest.len()) && is_token(rest))
}

/// `^msg_[A-Za-z0-9_]{1,64}$` (2.1.316 `Dns`).
fn is_message_id(value: &str) -> bool {
    value.strip_prefix("msg_").is_some_and(|rest| {
        (1..=64).contains(&rest.len())
            && rest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

/// `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`.
fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// How many user turns carry text (a string, or a `text` block) rather than
/// only tool results: the ordinal of the prompt the request belongs to.
fn user_text_turns(body: &Value) -> usize {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return 0;
    };
    messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .filter(|message| match message.get("content") {
            Some(Value::String(text)) => !text.is_empty(),
            Some(Value::Array(blocks)) => blocks
                .iter()
                .any(|block| block.get("type").and_then(Value::as_str) == Some("text")),
            _ => false,
        })
        .count()
}

/// The CLI hashes its first non-meta user message (2.1.311 `dH`). Meta
/// context reaches the wire as `<system-reminder>` text merged ahead of the
/// prompt, so those blocks are skipped.
fn first_user_text(body: &Value) -> &str {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return "";
    };
    let prompt = |text: &&str| !text.starts_with("<system-reminder>");
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = message.get("content") else {
            continue;
        };
        if let Some(text) = content.as_str().filter(prompt) {
            return text;
        }
        if let Some(text) = content.as_array().and_then(|blocks| {
            blocks.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
                    .filter(prompt)
            })
        }) {
            return text;
        }
    }
    ""
}

/// Three hex characters from the salt, three UTF-16 units of the first user
/// text and the CLI version (v3 `cch.rs`).
fn version_suffix(text: &str) -> String {
    let code_units = text.encode_utf16().collect::<Vec<_>>();
    let selected = [4_usize, 7, 20]
        .into_iter()
        .map(|index| code_units.get(index).copied().unwrap_or(u16::from(b'0')))
        .collect::<Vec<_>>();
    let selected = String::from_utf16_lossy(&selected);
    let mut hasher = Sha256::new();
    hasher.update(SUFFIX_SALT.as_bytes());
    hasher.update(selected.as_bytes());
    hasher.update(super::CLI_VERSION.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
        .chars()
        .take(3)
        .collect()
}

// -------------------------------------------------------------- sampling

fn strip_sampling(body: &mut Value) {
    if let Some(root) = body.as_object_mut() {
        for name in ["temperature", "top_p", "top_k"] {
            root.remove(name);
        }
    }
}

/// A trailing assistant text turn is a prefill; models that reject prefills
/// get it as a user turn instead. Thinking or tool blocks are left alone.
fn coerce_prefill(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    if !model.contains("claude") || PREFILL_TOLERANT.iter().any(|value| model.contains(value)) {
        return;
    }
    let Some(last) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.last_mut())
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let text_prefill = match last.get("content") {
        Some(Value::String(_)) => true,
        Some(Value::Array(blocks)) => {
            !blocks.is_empty()
                && blocks
                    .iter()
                    .all(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        }
        _ => false,
    };
    if text_prefill && last.get("role").and_then(Value::as_str) == Some("assistant") {
        last.insert("role".into(), Value::String("user".into()));
    }
}

// ----------------------------------------------------------------- betas

fn append_fast_beta(body: &Value, headers: &mut HeaderMap) {
    if body.get("speed").and_then(Value::as_str) == Some("fast") {
        append_beta(headers, FAST_MODE_BETA);
    }
}

fn append_thinking_display_beta(body: &Value, headers: &mut HeaderMap) {
    if body.pointer("/thinking/display").and_then(Value::as_str) == Some("updates") {
        append_beta(headers, THINKING_DISPLAY_UPDATES_BETA);
    }
}

fn append_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = beta_values(headers);
    if !values.iter().any(|value| value == beta) {
        values.push(beta.into());
    }
    write_beta(headers, values);
}

fn strip_beta(headers: &mut HeaderMap, beta: &str) {
    let values = beta_values(headers)
        .into_iter()
        .filter(|value| value != beta)
        .collect();
    write_beta(headers, values);
}

fn beta_values(headers: &HeaderMap) -> Vec<String> {
    headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn write_beta(headers: &mut HeaderMap, values: Vec<String>) {
    if values.is_empty() {
        headers.remove("anthropic-beta");
    } else if let Ok(value) = HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}
