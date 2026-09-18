//! In-place rewriting of JSON string values selected by dot paths. The text is
//! scanned, never parsed into a tree, so key order, whitespace and number
//! formatting survive untouched and only the selected strings are re-encoded.

use crate::PathSegment;

enum Frame {
    Object { key: Option<String>, expect: Expect },
    Array { index: usize, expect: Expect },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Object: a key or `}`; array: a value or `]`.
    First,
    Key,
    Colon,
    Value,
    CommaOrEnd,
}

enum Current<'a> {
    Key(&'a str),
    Index(usize),
}

/// Apply `rewrite` to every string value whose path matches one of `paths`.
/// Returns the rewritten text when at least one value changed. Malformed JSON
/// yields `None`: path rules are no-ops on payloads that are not JSON.
pub fn rewrite_at_paths(
    text: &str,
    paths: &[Vec<PathSegment>],
    rewrite: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<String> {
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = skip_ws(bytes, 0);
    // A scalar root can never match a non-empty path.
    if i >= bytes.len() || !matches!(bytes[i], b'{' | b'[') {
        return None;
    }
    loop {
        i = skip_ws(bytes, i);
        let &byte = bytes.get(i)?;
        let Some(frame) = stack.last_mut() else {
            // Root container start.
            stack.push(open(byte)?);
            i += 1;
            continue;
        };
        match frame {
            Frame::Object { key, expect } => match *expect {
                Expect::First | Expect::Key => {
                    if byte == b'}' && *expect == Expect::First {
                        stack.pop();
                        i += 1;
                        if stack.is_empty() {
                            break;
                        }
                        continue;
                    }
                    if byte != b'"' {
                        return None;
                    }
                    let end = string_end(bytes, i)?;
                    *key = Some(serde_json::from_str::<String>(&text[i..end]).ok()?);
                    *expect = Expect::Colon;
                    i = end;
                }
                Expect::Colon => {
                    if byte != b':' {
                        return None;
                    }
                    *expect = Expect::Value;
                    i += 1;
                }
                Expect::Value => {
                    *expect = Expect::CommaOrEnd;
                    match byte {
                        b'{' | b'[' => {
                            stack.push(open(byte)?);
                            i += 1;
                        }
                        b'"' => {
                            let end = string_end(bytes, i)?;
                            i = consider_string(text, &stack, paths, i, end, rewrite, &mut edits);
                        }
                        _ => i = scalar_end(bytes, i)?,
                    }
                }
                Expect::CommaOrEnd => match byte {
                    b',' => {
                        *expect = Expect::Key;
                        i += 1;
                    }
                    b'}' => {
                        stack.pop();
                        i += 1;
                        if stack.is_empty() {
                            break;
                        }
                    }
                    _ => return None,
                },
            },
            Frame::Array { index, expect } => match *expect {
                Expect::First | Expect::Value => {
                    if byte == b']' && *expect == Expect::First {
                        stack.pop();
                        i += 1;
                        if stack.is_empty() {
                            break;
                        }
                        continue;
                    }
                    *expect = Expect::CommaOrEnd;
                    match byte {
                        b'{' | b'[' => {
                            stack.push(open(byte)?);
                            i += 1;
                        }
                        b'"' => {
                            let end = string_end(bytes, i)?;
                            i = consider_string(text, &stack, paths, i, end, rewrite, &mut edits);
                        }
                        _ => i = scalar_end(bytes, i)?,
                    }
                }
                Expect::CommaOrEnd => match byte {
                    b',' => {
                        *index += 1;
                        *expect = Expect::Value;
                        i += 1;
                    }
                    b']' => {
                        stack.pop();
                        i += 1;
                        if stack.is_empty() {
                            break;
                        }
                    }
                    _ => return None,
                },
                Expect::Key | Expect::Colon => return None,
            },
        }
    }
    if skip_ws(bytes, i) != bytes.len() {
        return None;
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        out.push_str(&text[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

fn open(byte: u8) -> Option<Frame> {
    match byte {
        b'{' => Some(Frame::Object {
            key: None,
            expect: Expect::First,
        }),
        b'[' => Some(Frame::Array {
            index: 0,
            expect: Expect::First,
        }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn consider_string(
    text: &str,
    stack: &[Frame],
    paths: &[Vec<PathSegment>],
    start: usize,
    end: usize,
    rewrite: &mut dyn FnMut(&str) -> Option<String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> usize {
    let current: Vec<Current<'_>> = stack
        .iter()
        .map(|frame| match frame {
            Frame::Object { key, .. } => Current::Key(key.as_deref().unwrap_or("")),
            Frame::Array { index, .. } => Current::Index(*index),
        })
        .collect();
    if paths.iter().any(|path| path_matches(path, &current))
        && let Ok(value) = serde_json::from_str::<String>(&text[start..end])
        && let Some(replaced) = rewrite(&value)
        && let Ok(encoded) = serde_json::to_string(&replaced)
    {
        edits.push((start, end, encoded));
    }
    end
}

fn path_matches(path: &[PathSegment], current: &[Current<'_>]) -> bool {
    path.len() == current.len()
        && path
            .iter()
            .zip(current)
            .all(|(segment, here)| match (segment, here) {
                (PathSegment::Wildcard, _) => true,
                (PathSegment::Key(key), Current::Key(here)) => key == here,
                (PathSegment::Index(index), Current::Index(here)) => index == here,
                _ => false,
            })
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// `bytes[start] == b'"'`; returns the index just past the closing quote.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Numbers, `true`, `false`, `null`: run to the next structural delimiter.
fn scalar_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len()
        && !matches!(bytes[i], b',' | b'}' | b']')
        && !bytes[i].is_ascii_whitespace()
    {
        i += 1;
    }
    (i > start).then_some(i)
}
