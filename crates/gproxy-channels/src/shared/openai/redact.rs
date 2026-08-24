//! Incremental redaction for image stream payload observation.

#[derive(Default)]
pub(crate) struct B64Redactor {
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Normal,
    String {
        matched: usize,
        possible_key: bool,
        escaped: bool,
    },
    AfterKey,
    AfterColon,
    Redacting {
        escaped: bool,
    },
}

const KEY: &[u8] = b"b64_json";

impl B64Redactor {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(chunk.len().min(8 * 1024));
        for &byte in chunk {
            self.state = match std::mem::take(&mut self.state) {
                State::Normal => normal(byte, &mut output),
                State::String {
                    mut matched,
                    mut possible_key,
                    mut escaped,
                } => {
                    output.push(byte);
                    if escaped {
                        escaped = false;
                        possible_key = false;
                        State::String {
                            matched,
                            possible_key,
                            escaped,
                        }
                    } else if byte == b'\\' {
                        escaped = true;
                        possible_key = false;
                        State::String {
                            matched,
                            possible_key,
                            escaped,
                        }
                    } else if byte == b'"' {
                        if possible_key && matched == KEY.len() {
                            State::AfterKey
                        } else {
                            State::Normal
                        }
                    } else {
                        possible_key &= KEY.get(matched) == Some(&byte);
                        matched = matched.saturating_add(1);
                        State::String {
                            matched,
                            possible_key,
                            escaped,
                        }
                    }
                }
                State::AfterKey => {
                    output.push(byte);
                    if byte.is_ascii_whitespace() {
                        State::AfterKey
                    } else if byte == b':' {
                        State::AfterColon
                    } else {
                        after(byte)
                    }
                }
                State::AfterColon => {
                    output.push(byte);
                    if byte.is_ascii_whitespace() {
                        State::AfterColon
                    } else if byte == b'"' {
                        State::Redacting { escaped: false }
                    } else {
                        after(byte)
                    }
                }
                State::Redacting { mut escaped } => {
                    if escaped {
                        escaped = false;
                        State::Redacting { escaped }
                    } else if byte == b'\\' {
                        State::Redacting { escaped: true }
                    } else if byte == b'"' {
                        output.push(byte);
                        State::Normal
                    } else {
                        State::Redacting { escaped }
                    }
                }
            };
        }
        output
    }
}

fn normal(byte: u8, output: &mut Vec<u8>) -> State {
    output.push(byte);
    after(byte)
}

fn after(byte: u8) -> State {
    if byte == b'"' {
        State::String {
            matched: 0,
            possible_key: true,
            escaped: false,
        }
    } else {
        State::Normal
    }
}
