//! Reversible client aliases for tool calls.
//!
//! A converted response usually forwards the upstream's own call ID. When it
//! cannot, because the upstream sent no ID, the ID is not valid for the
//! client's dialect, or a second fanout candidate repeats it, the client gets
//! an alias instead. The alias itself carries everything a later turn needs to
//! reach the upstream again, so nothing is recorded when it is emitted and
//! nothing is looked up when the client sends it back.
//!
//! Every alias starts with the client dialect's call prefix (`toolu_` for
//! Claude, `call_` elsewhere) followed by a `gp` marker:
//!
//! - `{prefix}gpe_{escaped}` is an upstream ID escaped into the client's
//!   alphabet, and `{prefix}gpe{n}_{escaped}` is its `n`th repetition;
//! - `{prefix}gpn_{salt}_{index}` is a call the upstream sent without an ID;
//! - `{prefix}gpl_{salt}_{index}` is a legacy Chat `function_call`, which has
//!   no ID either but has to be sent back in its own form.
//!
//! The escaped form keeps ASCII letters, digits and `-`, writes `_` as `__`,
//! and writes every other byte as `_` and two lowercase hex digits, so it only
//! ever uses `[A-Za-z0-9_-]`, Claude's `tool_use.id` alphabet. The salt is the
//! response's identity namespace, which keeps the aliases of two responses in
//! one conversation apart; the index is the call's position in its response.
//!
//! Decoding accepts only the canonical form of each alias, so decoding is a
//! bijection with encoding. An upstream ID that happens to decode is never
//! forwarded as-is: it is escaped like an invalid one, so a string the client
//! sends back decodes exactly when gproxy made it an alias.

use crate::Dialect;
use std::fmt::Write;

const PREFIXES: [&str; 2] = ["toolu_", "call_"];

/// What a client alias stands for on the upstream side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAlias {
    /// The upstream's exact ID. `repeat` is zero unless an earlier call in the
    /// same response already went out under that ID.
    Upstream { id: String, repeat: u64 },
    /// The upstream sent the call without an ID. A legacy Chat call must be
    /// replayed as a `function_call`; the tool name comes from the client's
    /// own history.
    Missing {
        legacy: bool,
        salt: String,
        index: u64,
    },
}

/// Whether the client dialect accepts `id` as a tool call ID. Only Claude
/// restricts the alphabet; every other dialect takes any non-empty string.
pub fn accepts(dialect: Dialect, id: &str) -> bool {
    !id.is_empty()
        && (dialect != Dialect::Claude
            || id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

/// The alias for an upstream ID, or for its `repeat`th repetition.
pub fn upstream(prefix: &str, id: &str, repeat: u64) -> String {
    let mut alias = format!("{prefix}gpe");
    if repeat > 0 {
        let _ = write!(alias, "{repeat}");
    }
    alias.push('_');
    for byte in id.bytes() {
        match byte {
            b'_' => alias.push_str("__"),
            b if b.is_ascii_alphanumeric() || b == b'-' => alias.push(char::from(b)),
            b => {
                let _ = write!(alias, "_{b:02x}");
            }
        }
    }
    alias
}

/// The alias for a call the upstream sent without an ID.
pub fn missing(prefix: &str, salt: &str, index: u64, legacy: bool) -> String {
    let tag = if legacy { 'l' } else { 'n' };
    format!("{prefix}gp{tag}_{salt}_{index}")
}

/// What a client ID stands for, or `None` for an ID gproxy never aliased,
/// which goes upstream unchanged.
pub fn decode(alias: &str) -> Option<ToolAlias> {
    let rest = PREFIXES
        .iter()
        .find_map(|prefix| alias.strip_prefix(prefix))?
        .strip_prefix("gp")?;
    let (tag, rest) = rest.split_at_checked(1)?;
    let (count, body) = rest.split_once('_')?;
    match tag {
        "e" => {
            let repeat = if count.is_empty() {
                0
            } else {
                canonical_number(count).filter(|n| *n > 0)?
            };
            Some(ToolAlias::Upstream {
                id: unescape(body)?,
                repeat,
            })
        }
        "n" | "l" if count.is_empty() => {
            let (salt, index) = body.split_once('_')?;
            if salt.is_empty()
                || !salt
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return None;
            }
            Some(ToolAlias::Missing {
                legacy: tag == "l",
                salt: salt.to_owned(),
                index: canonical_number(index)?,
            })
        }
        _ => None,
    }
}

/// A decimal with no sign and no leading zero, so each number has one form.
fn canonical_number(value: &str) -> Option<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    value.parse().ok()
}

fn unescape(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(body.len());
    let mut input = body.bytes();
    while let Some(byte) = input.next() {
        match byte {
            b'_' => match input.next()? {
                b'_' => bytes.push(b'_'),
                high => {
                    let byte = (hex(high)? << 4) | hex(input.next()?)?;
                    // A byte the plain form carries as itself has exactly one
                    // spelling, so an escaped one is not an alias gproxy made.
                    if byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric() {
                        return None;
                    }
                    bytes.push(byte);
                }
            },
            b if b.is_ascii_alphanumeric() || b == b'-' => bytes.push(b),
            _ => return None,
        }
    }
    String::from_utf8(bytes).ok()
}

fn hex(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(id: &str, repeat: u64) -> Option<ToolAlias> {
        Some(ToolAlias::Upstream {
            id: id.into(),
            repeat,
        })
    }

    #[test]
    fn escaped_ids_round_trip_through_the_claude_alphabet() {
        for id in [
            "a.b",
            "tool:source",
            "plain",
            "under_score",
            "__",
            "_x2e",
            "-",
            "ü/日本",
            " ",
            "%",
            "toolu_gpe_x",
        ] {
            for repeat in [0, 1, 12] {
                let alias = upstream("toolu_", id, repeat);
                assert!(accepts(Dialect::Claude, &alias), "{alias}");
                assert_eq!(decode(&alias), up(id, repeat), "{alias}");
            }
        }
        assert_eq!(upstream("toolu_", "a.b", 0), "toolu_gpe_a_2eb");
        assert_eq!(upstream("call_", "a_b", 2), "call_gpe2_a__b");
        assert_eq!(
            upstream("toolu_", "tool:source", 0),
            "toolu_gpe_tool_3asource"
        );
    }

    #[test]
    fn missing_ids_carry_their_form_and_position() {
        let alias = missing("toolu_", "00ff", 3, false);
        assert_eq!(alias, "toolu_gpn_00ff_3");
        assert!(accepts(Dialect::Claude, &alias));
        assert_eq!(
            decode(&alias),
            Some(ToolAlias::Missing {
                legacy: false,
                salt: "00ff".into(),
                index: 3
            })
        );
        assert_eq!(
            decode(&missing("call_", "ab", 0, true)),
            Some(ToolAlias::Missing {
                legacy: true,
                salt: "ab".into(),
                index: 0
            })
        );
    }

    #[test]
    fn natural_ids_do_not_decode() {
        for id in [
            "",
            "toolu_01AbCdEf",
            "call_abc123",
            "call_gp",
            "call_gpe",
            "call_gpe_",
            "toolu_gpx_a",
            "gpe_abc",
            "fc_gpe_abc",
            "call_gpe0_a",
            "call_gpe01_a",
            "call_gpe_a_",
            "call_gpe_a_2",
            "call_gpe_a_2E",
            "call_gpe_a_41",
            "call_gpe_a_5f",
            "call_gpe_a.b",
            "call_gpe_a_ff",
            "call_gpn_ab",
            "call_gpn__1",
            "call_gpn_AB_1",
            "call_gpn_ab_01",
            "call_gpn_ab_x",
            "call_gpn1_ab_1",
            "call_gpl_ab_1_2",
        ] {
            assert_eq!(decode(id), None, "{id}");
        }
    }

    #[test]
    fn an_id_that_looks_like_an_alias_is_escaped_apart_from_it() {
        let natural = "call_gpe_x";
        assert_eq!(decode(natural), up("x", 0));
        let alias = upstream("call_", natural, 0);
        assert_ne!(alias, natural);
        assert_eq!(decode(&alias), up(natural, 0));
        let missing = missing("call_", "ab", 0, false);
        assert_eq!(decode(&upstream("call_", &missing, 0)), up(&missing, 0));
    }

    #[test]
    fn only_claude_restricts_the_alphabet() {
        assert!(!accepts(Dialect::Claude, "a.b"));
        assert!(!accepts(Dialect::Claude, ""));
        assert!(accepts(Dialect::Claude, "toolu_01-x_Y"));
        assert!(accepts(Dialect::OpenAiChat, "a.b:c"));
        assert!(accepts(Dialect::Gemini, "a b"));
        assert!(!accepts(Dialect::OpenAi, ""));
    }
}
