use bytes::Bytes;

#[derive(Debug, Clone, Default)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
    event: Option<String>,
    data_lines: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_bytes(&mut self, chunk: &Bytes) -> Vec<SseEvent> {
        match std::str::from_utf8(chunk) {
            Ok(text) => self.push_str(text),
            Err(_) => Vec::new(),
        }
    }

    pub fn push_str(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        while let Some(pos) = self.buffer.find('\n') {
            let mut line = self.buffer[..pos].to_string();
            self.buffer.drain(..=pos);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                self.finish_event(&mut events);
                continue;
            }

            if line.starts_with(':') {
                continue;
            }

            if let Some(value) = line.strip_prefix("event:") {
                let value = value.trim_start();
                self.event = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
                continue;
            }
            if line == "event" {
                self.event = None;
                continue;
            }

            if let Some(value) = line.strip_prefix("data:") {
                let value = value.trim_start();
                self.data_lines.push(value.to_string());
                continue;
            }
            if line == "data" {
                self.data_lines.push(String::new());
                continue;
            }
        }

        events
    }

    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let mut line = std::mem::take(&mut self.buffer);
            if line.ends_with('\r') {
                line.pop();
            }
            if let Some(value) = line.strip_prefix("event:") {
                let value = value.trim_start();
                self.event = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            } else if let Some(value) = line.strip_prefix("data:") {
                let value = value.trim_start();
                self.data_lines.push(value.to_string());
            }
        }
        self.finish_event(&mut events);
        events
    }

    fn finish_event(&mut self, events: &mut Vec<SseEvent>) {
        if self.event.is_none() && self.data_lines.is_empty() {
            return;
        }
        let data = self.data_lines.join("\n");
        events.push(SseEvent {
            event: self.event.take(),
            data,
        });
        self.data_lines.clear();
    }
}
