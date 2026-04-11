use gproxy_protocol::kinds::ProtocolKind;

use crate::request::PreparedRequest;

/// A single suffix modifier within a group.
pub struct SuffixEntry {
    /// The suffix segment, e.g. `"-fast"`, `"-thinking-high"`.
    pub suffix: &'static str,
    /// Modify the `PreparedRequest`.
    pub apply: fn(&mut PreparedRequest),
}

/// A group of mutually exclusive suffixes.
///
/// Only one suffix per group can match. Groups are matched right-to-left
/// from the model name, allowing combinations like `-thinking-high-fast`.
pub struct SuffixGroup {
    pub entries: &'static [SuffixEntry],
}

/// Result of matching multiple suffix groups against a model name.
pub struct MatchedSuffixes {
    /// The base model name with all suffixes stripped.
    pub base_model: String,
    /// The full combined suffix string (e.g. `"-thinking-high-fast"`).
    pub combined_suffix: String,
    /// Apply functions to call, in match order.
    pub apply_fns: Vec<fn(&mut PreparedRequest)>,
}

/// Get the protocol-level suffix groups for a given destination protocol.
pub fn suffix_groups_for_protocol(dst_proto: ProtocolKind) -> &'static [SuffixGroup] {
    match dst_proto {
        ProtocolKind::Claude => CLAUDE_SUFFIX_GROUPS,
        ProtocolKind::OpenAiResponse | ProtocolKind::OpenAi => OPENAI_RESPONSE_SUFFIX_GROUPS,
        ProtocolKind::OpenAiChatCompletion => OPENAI_CHAT_COMPLETIONS_SUFFIX_GROUPS,
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => GEMINI_SUFFIX_GROUPS,
    }
}

/// Match suffix groups against a model name.
pub fn match_suffix_groups(model: &str, groups: &[SuffixGroup]) -> Option<MatchedSuffixes> {
    let mut remaining = model;
    let mut apply_fns = Vec::new();
    let mut total_stripped = 0usize;

    let mut matched_any = true;
    while matched_any {
        matched_any = false;
        for group in groups {
            let mut best: Option<&SuffixEntry> = None;
            for entry in group.entries {
                if remaining.ends_with(entry.suffix)
                    && best.is_none_or(|b| entry.suffix.len() > b.suffix.len())
                {
                    best = Some(entry);
                }
            }
            if let Some(entry) = best {
                remaining = &remaining[..remaining.len() - entry.suffix.len()];
                total_stripped += entry.suffix.len();
                apply_fns.push(entry.apply);
                matched_any = true;
                break;
            }
        }
    }

    if total_stripped == 0 {
        return None;
    }

    Some(MatchedSuffixes {
        base_model: remaining.to_string(),
        combined_suffix: model[model.len() - total_stripped..].to_string(),
        apply_fns,
    })
}

/// Strip all matched suffixes from the `"model"` field inside a JSON body,
/// replacing it with `base_model`.
pub fn strip_model_suffix_in_body(body: &mut Vec<u8>, base_model: &str) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    if v.get("model").and_then(|m| m.as_str()).is_some() {
        v["model"] = serde_json::Value::String(base_model.to_string());
        if let Ok(b) = serde_json::to_vec(&v) {
            *body = b;
        }
    }
}

/// Append the combined suffix to the `"model"` field inside a JSON body.
pub fn rewrite_model_suffix_in_body(body: &mut Vec<u8>, suffix: &str) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    if let Some(m) = v
        .get_mut("model")
        .and_then(|m| m.as_str().map(String::from))
    {
        v["model"] = serde_json::Value::String(format!("{}{}", m, suffix));
        if let Ok(b) = serde_json::to_vec(&v) {
            *body = b;
        }
    }
}

/// Append the combined suffix to the model identifier inside a model-get
/// response body. The field name depends on the protocol:
/// - Claude / OpenAI: `"id"`
/// - Gemini: `"name"` and `"baseModelId"`
pub fn rewrite_model_get_suffix_in_body(
    body: &mut Vec<u8>,
    suffix: &str,
    response_proto: ProtocolKind,
) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    match response_proto {
        ProtocolKind::Claude
        | ProtocolKind::OpenAi
        | ProtocolKind::OpenAiResponse
        | ProtocolKind::OpenAiChatCompletion => {
            if let Some(id) = v.get("id").and_then(|i| i.as_str()).map(String::from) {
                v["id"] = serde_json::Value::String(format!("{id}{suffix}"));
            }
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            if let Some(name) = v.get("name").and_then(|n| n.as_str()).map(String::from) {
                v["name"] = serde_json::Value::String(format!("{name}{suffix}"));
            }
            if let Some(base) = v
                .get("baseModelId")
                .and_then(|b| b.as_str())
                .map(String::from)
            {
                v["baseModelId"] = serde_json::Value::String(format!("{base}{suffix}"));
            }
        }
    }
    if let Ok(b) = serde_json::to_vec(&v) {
        *body = b;
    }
}

/// Expand a model-list response body by appending suffixed copies of each
/// model entry. The response is already in the client's protocol format.
pub fn expand_model_list_with_suffixes(
    body: &mut Vec<u8>,
    response_proto: ProtocolKind,
    groups: &[SuffixGroup],
) {
    // Collect all individual suffixes from the groups.
    let mut suffixes: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in groups.iter() {
        for entry in group.entries {
            if seen.insert(entry.suffix) {
                suffixes.push(entry.suffix);
            }
        }
    }
    if suffixes.is_empty() {
        return;
    }

    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };

    let (array_key, id_key) = match response_proto {
        ProtocolKind::Claude
        | ProtocolKind::OpenAi
        | ProtocolKind::OpenAiResponse
        | ProtocolKind::OpenAiChatCompletion => ("data", "id"),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => ("models", "name"),
    };

    let Some(arr) = v.get_mut(array_key).and_then(|a| a.as_array_mut()) else {
        return;
    };

    let originals: Vec<serde_json::Value> = arr.clone();
    for suffix in &suffixes {
        for model in &originals {
            let Some(id) = model.get(id_key).and_then(|i| i.as_str()) else {
                continue;
            };
            let mut copy = model.clone();
            copy[id_key] = serde_json::Value::String(format!("{id}{suffix}"));
            // For Gemini, also suffix baseModelId.
            if matches!(
                response_proto,
                ProtocolKind::Gemini | ProtocolKind::GeminiNDJson
            ) {
                if let Some(base) = model.get("baseModelId").and_then(|b| b.as_str()) {
                    copy["baseModelId"] = serde_json::Value::String(format!("{base}{suffix}"));
                }
            }
            arr.push(copy);
        }
    }

    if let Ok(b) = serde_json::to_vec(&v) {
        *body = b;
    }
}

// ---------------------------------------------------------------------------
// Body mutation helper
// ---------------------------------------------------------------------------

fn mutate_body(req: &mut PreparedRequest, f: impl FnOnce(&mut serde_json::Value)) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&req.body) else {
        return;
    };
    f(&mut v);
    if let Ok(b) = serde_json::to_vec(&v) {
        req.body = b;
    }
}

/// Remove any `anthropic-beta` header value containing the given prefix.
/// If multiple beta values are comma-separated, filters out only the matching one.
fn remove_beta_header(req: &mut PreparedRequest, prefix: &str) {
    let Some(current) = req
        .headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok().map(String::from))
    else {
        return;
    };
    let filtered: Vec<&str> = current
        .split(',')
        .map(str::trim)
        .filter(|v| !v.starts_with(prefix))
        .collect();
    if filtered.is_empty() {
        req.headers.remove("anthropic-beta");
    } else if let Ok(val) = http::HeaderValue::from_str(&filtered.join(", ")) {
        req.headers.insert("anthropic-beta", val);
    }
}

// ===========================================================================
// Claude suffix groups
// ===========================================================================

static CLAUDE_THINKING: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-thinking-none",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinking"] = serde_json::json!({"type": "disabled"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": 1024});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": 10240});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": 32768});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-max",
            apply: |req| {
                mutate_body(req, |v| {
                    let budget = v
                        .get("max_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(128000);
                    v["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": budget});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-adaptive",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinking"] = serde_json::json!({"type": "adaptive"});
                });
            },
        },
    ],
};

static CLAUDE_SPEED: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-fast",
            apply: |req| {
                mutate_body(req, |v| {
                    v["speed"] = serde_json::json!("fast");
                });
                req.headers.insert(
                    "anthropic-beta",
                    http::HeaderValue::from_static("fast-mode-2026-02-01"),
                );
            },
        },
        SuffixEntry {
            suffix: "-non-fast",
            apply: |req| {
                // Explicitly remove fast mode: set speed to standard and drop fast-mode beta
                mutate_body(req, |v| {
                    if let Some(obj) = v.as_object_mut() {
                        obj.remove("speed");
                    }
                });
                req.headers.remove("anthropic-beta");
            },
        },
    ],
};

static CLAUDE_CONTEXT: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-1m",
            apply: |req| {
                req.headers.insert(
                    "anthropic-beta",
                    http::HeaderValue::from_static("context-1m-2025-08-07"),
                );
            },
        },
        SuffixEntry {
            suffix: "-200k",
            apply: |req| {
                // Actively remove context-1m beta header if present
                remove_beta_header(req, "context-1m");
            },
        },
    ],
};

static CLAUDE_EFFORT: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-effort-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["output_config"] = serde_json::json!({"effort": "low"});
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["output_config"] = serde_json::json!({"effort": "medium"});
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["output_config"] = serde_json::json!({"effort": "high"});
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-max",
            apply: |req| {
                mutate_body(req, |v| {
                    v["output_config"] = serde_json::json!({"effort": "max"});
                });
            },
        },
    ],
};

pub static CLAUDE_SUFFIX_GROUPS: &[SuffixGroup] = &[
    SuffixGroup {
        entries: CLAUDE_THINKING.entries,
    },
    SuffixGroup {
        entries: CLAUDE_SPEED.entries,
    },
    SuffixGroup {
        entries: CLAUDE_CONTEXT.entries,
    },
    SuffixGroup {
        entries: CLAUDE_EFFORT.entries,
    },
];

// ===========================================================================
// OpenAI Response API suffix groups
// ===========================================================================

static OPENAI_RESPONSE_THINKING: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-thinking-none",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning"] = serde_json::json!({"effort": "none"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning"] = serde_json::json!({"effort": "low"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning"] = serde_json::json!({"effort": "medium"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning"] = serde_json::json!({"effort": "high"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-xhigh",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning"] = serde_json::json!({"effort": "xhigh"});
                });
            },
        },
    ],
};

static OPENAI_RESPONSE_SPEED: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-tier-auto",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("auto");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-default",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("default");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-flex",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("flex");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-scale",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("scale");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-priority",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("priority");
                });
            },
        },
        SuffixEntry {
            suffix: "-fast",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("priority");
                });
            },
        },
        SuffixEntry {
            suffix: "-non-fast",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("default");
                });
            },
        },
    ],
};

static OPENAI_RESPONSE_EFFORT: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-effort-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["text"] = serde_json::json!({"verbosity": "low"});
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["text"] = serde_json::json!({"verbosity": "medium"});
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["text"] = serde_json::json!({"verbosity": "high"});
                });
            },
        },
    ],
};

pub static OPENAI_RESPONSE_SUFFIX_GROUPS: &[SuffixGroup] = &[
    SuffixGroup {
        entries: OPENAI_RESPONSE_THINKING.entries,
    },
    SuffixGroup {
        entries: OPENAI_RESPONSE_SPEED.entries,
    },
    SuffixGroup {
        entries: OPENAI_RESPONSE_EFFORT.entries,
    },
];

// ===========================================================================
// OpenAI Chat Completions suffix groups
// ===========================================================================

static OPENAI_CHAT_THINKING: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-thinking-none",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning_effort"] = serde_json::json!("none");
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning_effort"] = serde_json::json!("low");
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning_effort"] = serde_json::json!("medium");
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning_effort"] = serde_json::json!("high");
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-xhigh",
            apply: |req| {
                mutate_body(req, |v| {
                    v["reasoning_effort"] = serde_json::json!("xhigh");
                });
            },
        },
    ],
};

static OPENAI_CHAT_SPEED: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-tier-auto",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("auto");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-default",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("default");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-flex",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("flex");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-scale",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("scale");
                });
            },
        },
        SuffixEntry {
            suffix: "-tier-priority",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("priority");
                });
            },
        },
        SuffixEntry {
            suffix: "-fast",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("priority");
                });
            },
        },
        SuffixEntry {
            suffix: "-non-fast",
            apply: |req| {
                mutate_body(req, |v| {
                    v["service_tier"] = serde_json::json!("default");
                });
            },
        },
    ],
};

static OPENAI_CHAT_EFFORT: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-effort-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["verbosity"] = serde_json::json!("low");
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["verbosity"] = serde_json::json!("medium");
                });
            },
        },
        SuffixEntry {
            suffix: "-effort-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["verbosity"] = serde_json::json!("high");
                });
            },
        },
    ],
};

pub static OPENAI_CHAT_COMPLETIONS_SUFFIX_GROUPS: &[SuffixGroup] = &[
    SuffixGroup {
        entries: OPENAI_CHAT_THINKING.entries,
    },
    SuffixGroup {
        entries: OPENAI_CHAT_SPEED.entries,
    },
    SuffixGroup {
        entries: OPENAI_CHAT_EFFORT.entries,
    },
];

// ===========================================================================
// Gemini suffix groups
// ===========================================================================

static GEMINI_THINKING: SuffixGroup = SuffixGroup {
    entries: &[
        SuffixEntry {
            suffix: "-thinking-none",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinkingConfig"] = serde_json::json!({"thinkingLevel": "MINIMAL"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-low",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinkingConfig"] = serde_json::json!({"thinkingLevel": "LOW"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-medium",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinkingConfig"] = serde_json::json!({"thinkingLevel": "MEDIUM"});
                });
            },
        },
        SuffixEntry {
            suffix: "-thinking-high",
            apply: |req| {
                mutate_body(req, |v| {
                    v["thinkingConfig"] = serde_json::json!({"thinkingLevel": "HIGH"});
                });
            },
        },
    ],
};

pub static GEMINI_SUFFIX_GROUPS: &[SuffixGroup] = &[SuffixGroup {
    entries: GEMINI_THINKING.entries,
}];
