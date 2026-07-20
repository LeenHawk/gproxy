//! Conduit deep-research message synthesis state.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Converts whole-message and widget snapshots into incremental SSE-v1 deltas.
#[derive(Default)]
pub(in crate::channel::bulletins::chatgpt) struct MsgSynth {
    next_ch: u64,
    report_ch: Option<u64>,
    report_text: String,
    thoughts_emitted: HashMap<String, usize>,
    plan_seen: HashMap<String, String>,
    banner: bool,
}

impl MsgSynth {
    pub(super) fn push_messages(&mut self, messages: &[Value], out: &mut String) -> bool {
        self.ensure_banner(out);
        let mut done = false;
        for message in messages {
            let role = message.pointer("/author/role").and_then(Value::as_str);
            let content = message.get("content");
            let content_type = content
                .and_then(|value| value.get("content_type"))
                .and_then(Value::as_str);
            let recipient = message
                .get("recipient")
                .and_then(Value::as_str)
                .unwrap_or("all");
            match content_type {
                Some("thoughts") => {
                    let id = message.get("id").and_then(Value::as_str).unwrap_or("");
                    let Some(thoughts) = content
                        .and_then(|value| value.get("thoughts"))
                        .and_then(Value::as_array)
                    else {
                        continue;
                    };
                    let emitted = self.thoughts_emitted.get(id).copied().unwrap_or(0);
                    if thoughts.len() > emitted {
                        let channel = self.alloc_ch();
                        let add = json!({"v":{"message":{"author":{"role":"assistant"},
                            "content":{"content_type":"thoughts","thoughts":thoughts[emitted..]},
                            "status":"in_progress"}},"c":channel});
                        sse_event(out, &add);
                        self.thoughts_emitted.insert(id.to_string(), thoughts.len());
                    }
                }
                Some("text") if role == Some("assistant") && recipient == "all" => {
                    let text = content
                        .and_then(|value| value.pointer("/parts/0"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    self.emit_report(text, out);
                    if message.get("status").and_then(Value::as_str)
                        == Some("finished_successfully")
                    {
                        done = true;
                    }
                }
                _ => {}
            }
        }
        done
    }

    pub(super) fn push_widget(&mut self, update_content: &Value, out: &mut String) -> bool {
        let Some(updates) = update_content.get("updates").and_then(Value::as_array) else {
            return false;
        };
        let mut done = false;
        for update in updates {
            let widget = update.get("widget_state");
            if let Some(steps) = widget
                .and_then(|value| value.pointer("/plan/steps"))
                .and_then(Value::as_array)
            {
                for step in steps {
                    self.push_step(step, out);
                }
            }
            if let Some(report) = widget.and_then(|value| value.get("report_message")) {
                if let Some(text) = report.pointer("/content/parts/0").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    self.ensure_banner(out);
                    self.emit_report(text, out);
                }
                if report.get("status").and_then(Value::as_str) == Some("finished_successfully") {
                    done = true;
                }
            }
        }
        done
    }

    fn ensure_banner(&mut self, out: &mut String) {
        if !self.banner {
            out.push_str("event: delta_encoding\ndata: \"v1\"\n\n");
            self.banner = true;
        }
    }

    fn emit_report(&mut self, text: &str, out: &mut String) {
        let channel = match self.report_ch {
            Some(channel) => channel,
            None => {
                let channel = self.alloc_ch();
                self.report_ch = Some(channel);
                let add = json!({"v":{"message":{"author":{"role":"assistant"},
                    "content":{"content_type":"text","parts":[""]},
                    "status":"in_progress","metadata":{"model_slug":"gpt-5"}}},"c":channel});
                sse_event(out, &add);
                channel
            }
        };
        let delta = if text.starts_with(&self.report_text) {
            &text[self.report_text.len()..]
        } else {
            text
        };
        if !delta.is_empty() {
            sse_event(
                out,
                &json!({"p":"/message/content/parts/0","o":"append","v":delta,"c":channel}),
            );
        }
        self.report_text = text.to_string();
    }

    fn push_step(&mut self, step: &Value, out: &mut String) {
        let status = step.get("status").and_then(Value::as_str).unwrap_or("");
        if status.is_empty() || status == "pending" {
            return;
        }
        let id = step.get("id").and_then(Value::as_str).unwrap_or("");
        let text = step.get("text").and_then(Value::as_str).unwrap_or("");
        let reason = step.get("reason").and_then(Value::as_str).unwrap_or("");
        let signature = format!("{status}|{reason}");
        if self.plan_seen.get(id) == Some(&signature) {
            return;
        }
        self.plan_seen.insert(id.to_string(), signature);
        self.ensure_banner(out);
        let marker = if status == "completed" { "✓" } else { "…" };
        let summary = format!("{marker} {text}");
        let content = if reason.is_empty() { status } else { reason };
        let channel = self.alloc_ch();
        let add = json!({"v":{"message":{"author":{"role":"assistant"},
            "content":{"content_type":"thoughts","thoughts":[{"summary":summary,"content":content}]},
            "status":"in_progress"}},"c":channel});
        sse_event(out, &add);
    }

    fn alloc_ch(&mut self) -> u64 {
        let channel = self.next_ch;
        self.next_ch += 1;
        channel
    }
}

fn sse_event(out: &mut String, data: &Value) {
    out.push_str("event: delta\ndata: ");
    out.push_str(&data.to_string());
    out.push_str("\n\n");
}
