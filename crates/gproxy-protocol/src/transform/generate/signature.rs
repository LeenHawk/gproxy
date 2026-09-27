//! Upstream signatures carried by the client in its own protocol's field.
//!
//! A Gemini `thoughtSignature` or a Claude thinking `signature` is opaque to
//! the client and verified by the upstream that issued it, so the gateway
//! keeps nothing: it hands the native value to the client in the field its
//! protocol already has for reasoning state (a Claude thinking block's
//! `signature`, a Responses reasoning item's `encrypted_content`), and reads
//! it back from there on the next turn. The value is not encrypted or wrapped;
//! only a short prefix names its source, so a later turn can put it back where
//! that upstream expects it, or strip it when the conversation has moved to an
//! upstream that would reject it.
//!
//! - `gemini:<signature>` is the signature of a Gemini thought part. The
//!   carrier's text is the part's text, and the pair replays as one
//!   `{thought: true, text, thoughtSignature}` part: the only shape a Claude
//!   model behind Antigravity accepts. An empty signature marks thought text
//!   the upstream did not sign; it replays as nothing.
//! - `gemini-next:<signature>` is the signature of the Gemini part that the
//!   carrier precedes in the client's history (a `functionCall`, or a
//!   generated image), which has no field of its own for it. The carrier has
//!   no text.
//! - `claude:<signature>` is a Claude thinking block's signature, shown to a
//!   client whose reasoning field is not a Claude one.
//!
//! A Claude client's own Claude signature carries no prefix: it is native
//! there, and a Claude request routed to Anthropic as it is must not change.
//! Chat clients carry signatures in `reasoning_details`, whose `format` names
//! the source instead (see `reasoning_details`).

/// A Gemini thought part's signature.
pub const GEMINI: &str = "gemini:";
/// A Gemini signature for the part after the carrier.
pub const GEMINI_NEXT: &str = "gemini-next:";
/// A Claude thinking block's signature.
pub const CLAUDE: &str = "claude:";

/// A carried signature, with its source prefix removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried<'a> {
    /// A Gemini thought part's signature; empty when the thought was unsigned.
    GeminiThought(&'a str),
    /// The signature of the Gemini part the carrier precedes.
    GeminiNext(&'a str),
    /// A Claude thinking signature.
    Claude(&'a str),
}

impl<'a> Carried<'a> {
    /// The carried form of `value`, or `None` for a value without a known
    /// prefix (a native signature, or an upstream's own ciphertext).
    pub fn parse(value: &'a str) -> Option<Self> {
        if let Some(signature) = value.strip_prefix(GEMINI_NEXT) {
            Some(Self::GeminiNext(signature))
        } else if let Some(signature) = value.strip_prefix(GEMINI) {
            Some(Self::GeminiThought(signature))
        } else {
            value.strip_prefix(CLAUDE).map(Self::Claude)
        }
    }
    /// True when the value came from a Gemini upstream.
    pub fn is_gemini(self) -> bool {
        matches!(self, Self::GeminiThought(_) | Self::GeminiNext(_))
    }
}

/// True for a value that carries a Gemini signature in any form.
pub fn is_gemini(value: &str) -> bool {
    Carried::parse(value).is_some_and(Carried::is_gemini)
}

/// True for a value with any source prefix gproxy adds.
pub fn is_carried(value: &str) -> bool {
    Carried::parse(value).is_some()
}

/// The carried form of a Gemini thought part's signature; `None` marks an
/// unsigned thought.
pub fn gemini_thought(signature: Option<&str>) -> String {
    format!("{GEMINI}{}", signature.unwrap_or_default())
}

/// The carried form of the signature of the Gemini part after the carrier.
pub fn gemini_next(signature: &str) -> String {
    format!("{GEMINI_NEXT}{signature}")
}

/// The carried form of a Claude thinking signature.
pub fn claude(signature: &str) -> String {
    format!("{CLAUDE}{signature}")
}

/// A Responses request (or input token count) body without the reasoning
/// items whose `encrypted_content` carries another upstream's signature;
/// `None` when it holds none. This is for a request that reaches a Responses
/// upstream as it is, without conversion: that upstream decrypts its own
/// ciphertext only, and refuses a Claude or Gemini signature it finds there.
pub fn without_carried_reasoning(body: &[u8]) -> Option<Vec<u8>> {
    let needles = [GEMINI, GEMINI_NEXT, CLAUDE].map(str::as_bytes);
    if !needles
        .iter()
        .any(|needle| body.windows(needle.len()).any(|window| window == *needle))
    {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let items = value.get_mut("input")?.as_array_mut()?;
    let before = items.len();
    items.retain(|item| {
        item.get("type").and_then(serde_json::Value::as_str) != Some("reasoning")
            || !item
                .get("encrypted_content")
                .and_then(serde_json::Value::as_str)
                .is_some_and(is_carried)
    });
    if items.len() == before {
        return None;
    }
    serde_json::to_vec(&value).ok()
}
