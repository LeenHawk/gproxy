//! Which wire each Copilot model answers on.
//!
//! Copilot serves a model on the endpoints its catalogue entry names in
//! `supported_endpoints` (`/chat/completions`, `/responses`, `ws:/responses`,
//! `/v1/messages`). The newest GPT and Codex models answer only `/responses`
//! (on `/chat/completions`: "not accessible via the /chat/completions
//! endpoint", `unsupported_api_for_model`); a model whose entry names no
//! endpoints (GPT-4.1, GPT-4o and older) is served on Chat Completions alone
//! (`samples/magpie/internal/provider/account.go` `copilotAPIs`, #754).
//!
//! The host picks the wire before a request reaches the channel, from the
//! upstream model name alone, so the catalogue is remembered process-wide
//! each time the channel relays one. A model not seen in a catalogue yet is
//! guessed from its name: GPT-5 family and Codex models on `/responses`
//! (which every one of them answers), everything else on Chat Completions.

use gproxy_protocol::Dialect;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// The two wires this channel speaks, as a model's catalogue entry allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Wires {
    pub chat: bool,
    pub responses: bool,
}

static SEEN: LazyLock<RwLock<HashMap<String, Wires>>> = LazyLock::new(Default::default);

/// The wires a `supported_endpoints` list names; none named is Chat alone.
fn named(endpoints: Option<&Vec<Value>>) -> Wires {
    let mut wires = Wires {
        chat: false,
        responses: false,
    };
    for endpoint in endpoints.into_iter().flatten().filter_map(Value::as_str) {
        // The websocket variant is not an HTTP wire; `/v1/...` is the same
        // route under a versioned prefix.
        if endpoint.starts_with("ws:") {
            continue;
        }
        match endpoint.strip_prefix("/v1").unwrap_or(endpoint) {
            "/chat/completions" => wires.chat = true,
            "/responses" => wires.responses = true,
            _ => {}
        }
    }
    if !wires.chat && !wires.responses {
        wires.chat = true;
    }
    wires
}

/// Remember the wires of every model a `/models` reply lists.
pub(super) fn record(body: &[u8]) {
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return;
    };
    let Some(models) = payload.get("data").and_then(Value::as_array) else {
        return;
    };
    let found: Vec<(String, Wires)> = models
        .iter()
        .filter_map(|model| {
            let id = model.get("id").and_then(Value::as_str)?;
            let endpoints = model.get("supported_endpoints").and_then(Value::as_array);
            Some((id.to_owned(), named(endpoints)))
        })
        .collect();
    if found.is_empty() {
        return;
    }
    if let Ok(mut seen) = SEEN.write() {
        seen.extend(found);
    }
}

/// A model no catalogue has listed yet.
fn guess(model: &str) -> Wires {
    let id = model.to_ascii_lowercase();
    let responses = id.starts_with("gpt-5") || id.contains("codex");
    Wires {
        chat: !responses,
        responses,
    }
}

fn of(model: &str) -> Wires {
    SEEN.read()
        .ok()
        .and_then(|seen| seen.get(model).copied())
        .unwrap_or_else(|| guess(model))
}

/// The generation dialects `model` is served on; without a model, both.
pub(super) fn dialects(model: Option<&str>) -> Vec<Dialect> {
    let wires = model.map_or(
        Wires {
            chat: true,
            responses: true,
        },
        of,
    );
    let mut dialects = Vec::with_capacity(2);
    if wires.chat {
        dialects.push(Dialect::OpenAiChat);
    }
    if wires.responses {
        dialects.push(Dialect::OpenAi);
    }
    dialects
}

/// The wire a client of neither OpenAI dialect is converted to when the
/// model answers both: Responses, which carries reasoning items (and their
/// encrypted content) across turns where Chat Completions has no place for
/// them.
pub(super) fn preferred(model: Option<&str>) -> Dialect {
    if dialects(model).contains(&Dialect::OpenAi) {
        Dialect::OpenAi
    } else {
        Dialect::OpenAiChat
    }
}
