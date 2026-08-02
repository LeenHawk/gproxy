//! Small `tracing` subscriber for console-based wasm edge runtimes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_core::span::Current;
use wasm_bindgen::prelude::*;

mod fields;

use fields::Fields;

static INSTALL: Once = Once::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static SPANS: RefCell<HashMap<u64, SpanData>> = RefCell::new(HashMap::new());
    static STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

#[derive(Clone, Copy)]
enum Filter {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Filter {
    fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("off") => Self::Off,
            Some("error") => Self::Error,
            Some("warn") => Self::Warn,
            Some("debug") => Self::Debug,
            Some("trace") => Self::Trace,
            _ => Self::Info,
        }
    }

    fn allows(self, level: &Level) -> bool {
        matches!(
            (self, *level),
            (Self::Error, Level::ERROR)
                | (Self::Warn, Level::ERROR | Level::WARN)
                | (Self::Info, Level::ERROR | Level::WARN | Level::INFO)
                | (
                    Self::Debug,
                    Level::ERROR | Level::WARN | Level::INFO | Level::DEBUG
                )
                | (Self::Trace, _)
        )
    }
}

struct SpanData {
    metadata: &'static Metadata<'static>,
    fields: Fields,
    refs: usize,
}

struct ConsoleSubscriber(Filter);

impl Subscriber for ConsoleSubscriber {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.0.allows(metadata.level())
    }

    fn new_span(&self, attrs: &Attributes<'_>) -> Id {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed).max(1);
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        SPANS.with(|spans| {
            spans.borrow_mut().insert(
                id,
                SpanData {
                    metadata: attrs.metadata(),
                    fields,
                    refs: 1,
                },
            );
        });
        Id::from_u64(id)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        SPANS.with(|spans| {
            if let Some(data) = spans.borrow_mut().get_mut(&span.into_u64()) {
                values.record(&mut data.fields);
            }
        });
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let metadata = event.metadata();
        let mut output = format!("{} {}", metadata.level(), metadata.target());
        if let Some(message) = fields.message() {
            output.push_str(": ");
            output.push_str(message);
        }
        STACK.with(|stack| {
            SPANS.with(|spans| {
                let spans = spans.borrow();
                for id in stack.borrow().iter() {
                    if let Some(span) = spans.get(id) {
                        span.fields.append_to(&mut output);
                    }
                }
            });
        });
        fields.append_to(&mut output);
        match *metadata.level() {
            Level::ERROR => console_error(&output),
            Level::WARN => console_warn(&output),
            _ => console_log(&output),
        }
    }

    fn enter(&self, span: &Id) {
        STACK.with(|stack| stack.borrow_mut().push(span.into_u64()));
    }

    fn exit(&self, span: &Id) {
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(index) = stack.iter().rposition(|id| *id == span.into_u64()) {
                stack.remove(index);
            }
        });
    }

    fn clone_span(&self, id: &Id) -> Id {
        SPANS.with(|spans| {
            if let Some(data) = spans.borrow_mut().get_mut(&id.into_u64()) {
                data.refs += 1;
            }
        });
        id.clone()
    }

    fn try_close(&self, id: Id) -> bool {
        SPANS.with(|spans| {
            let mut spans = spans.borrow_mut();
            let Some(data) = spans.get_mut(&id.into_u64()) else {
                return false;
            };
            if data.refs > 1 {
                data.refs -= 1;
                false
            } else {
                spans.remove(&id.into_u64());
                true
            }
        })
    }

    fn current_span(&self) -> Current {
        STACK.with(|stack| {
            SPANS.with(|spans| {
                let id = stack.borrow().last().copied();
                id.and_then(|id| {
                    spans
                        .borrow()
                        .get(&id)
                        .map(|span| Current::new(Id::from_u64(id), span.metadata))
                })
                .unwrap_or_else(Current::none)
            })
        })
    }
}

pub(super) fn init(filter: Option<&str>) {
    INSTALL.call_once(|| {
        let _ = tracing::subscriber::set_global_default(ConsoleSubscriber(Filter::parse(filter)));
    });
}
