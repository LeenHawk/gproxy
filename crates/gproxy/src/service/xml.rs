//! A well-formedness check for the two XML documents this module generates.
//!
//! # Why this is here rather than a dependency
//!
//! Two of the four platforms want XML: a launchd plist and a Task Scheduler
//! task. Neither is *parsed* by gproxy — both are handed to a tool that parses
//! them on another machine, days later, and both tools report a malformed
//! document uselessly. `launchctl bootstrap` says
//! `Bootstrap failed: 5: Input/output error`; `schtasks /create /xml` says
//! `The task XML is malformed` with no line number. So the document has to be
//! checked where it is written, and the check has to run on a Linux CI box
//! that has neither tool.
//!
//! A full parser as a dependency would be the obvious answer, and it is the
//! wrong one: it would be a build-time cost on every platform for a test-only
//! assertion about two documents whose shape this crate itself decides. This
//! is a hundred lines, it is compiled only under `cfg(test)`, and it catches
//! the three mistakes a generator actually makes — an unbalanced tag, an
//! unescaped `&` or `<` in a path, and an unquoted attribute.
//!
//! It is **not** a conforming XML parser. It does not resolve namespaces,
//! validate against the DTD it sees, or handle CDATA — none of which either
//! generator emits.

/// `Ok(())` if `text` is a well-formed XML document of the shape these
/// generators produce.
pub fn check(text: &str) -> Result<(), String> {
    let bytes: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut roots = 0;

    while at < bytes.len() {
        match bytes[at] {
            '<' => {
                let end = find(&bytes, at, '>')
                    .ok_or_else(|| format!("unterminated `<` at character {at}"))?;
                let tag: String = bytes[at + 1..end].iter().collect();
                at = end + 1;
                // A declaration, a doctype or a comment: skipped, but a
                // comment has to be skipped to its own `-->` rather than to
                // the first `>` inside it.
                if let Some(rest) = tag.strip_prefix("!--") {
                    if !rest.ends_with("--") {
                        let close = text
                            .get(end..)
                            .and_then(|rest| rest.find("-->"))
                            .ok_or_else(|| "unterminated comment".to_owned())?;
                        at = text[..end].chars().count() + close + 3;
                    }
                    continue;
                }
                if tag.starts_with('?') || tag.starts_with('!') {
                    continue;
                }

                if let Some(name) = tag.strip_prefix('/') {
                    let name = name.trim();
                    match stack.pop() {
                        Some(open) if open == name => {}
                        Some(open) => {
                            return Err(format!("`</{name}>` closes `<{open}>`"));
                        }
                        None => return Err(format!("`</{name}>` closes nothing")),
                    }
                    if stack.is_empty() {
                        roots += 1;
                    }
                    continue;
                }

                let selfclosing = tag.ends_with('/');
                let body = tag.trim_end_matches('/');
                let name = body
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| "an empty tag `<>`".to_owned())?;
                if name.is_empty() {
                    return Err("an empty tag `<>`".to_owned());
                }
                attributes(body.strip_prefix(name).unwrap_or(""))?;
                if selfclosing {
                    if stack.is_empty() {
                        roots += 1;
                    }
                } else {
                    stack.push(name.to_owned());
                }
            }
            '&' => {
                let end =
                    find(&bytes, at, ';').ok_or_else(|| format!("a bare `&` at character {at}"))?;
                let entity: String = bytes[at + 1..end].iter().collect();
                let known = matches!(entity.as_str(), "amp" | "lt" | "gt" | "quot" | "apos")
                    || entity.starts_with('#');
                if !known {
                    return Err(format!("`&{entity};` is not an XML entity"));
                }
                at = end + 1;
            }
            // A bare `>` in text is legal XML, so it is not an error here.
            _ => at += 1,
        }
    }

    if let Some(open) = stack.last() {
        return Err(format!("`<{open}>` is never closed"));
    }
    match roots {
        1 => Ok(()),
        0 => Err("no root element".to_owned()),
        many => Err(format!("{many} root elements; XML allows one")),
    }
}

/// Every attribute in a start tag is `name="value"` or `name='value'`.
fn attributes(mut rest: &str) -> Result<(), String> {
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(());
        }
        let (name, tail) = rest
            .split_once('=')
            .ok_or_else(|| format!("`{rest}` is not an attribute"))?;
        if name.trim().is_empty() || name.contains(char::is_whitespace) {
            return Err(format!("`{name}` is not an attribute name"));
        }
        let tail = tail.trim_start();
        let quote = tail
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| format!("the value of `{name}` is not quoted"))?;
        let close = tail[1..]
            .find(quote)
            .ok_or_else(|| format!("the value of `{name}` is not closed"))?;
        rest = &tail[1 + close + 1..];
    }
}

fn find(chars: &[char], from: usize, needle: char) -> Option<usize> {
    chars[from..]
        .iter()
        .position(|c| *c == needle)
        .map(|offset| from + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_document_passes() {
        check("<?xml version=\"1.0\"?>\n<a x=\"1\"><b/>text &amp; more</a>\n").unwrap();
        check("<!DOCTYPE plist PUBLIC \"x\" \"y\">\n<plist version=\"1.0\"><dict/></plist>")
            .unwrap();
        check("<a><!-- a comment with <brackets> in it --></a>").unwrap();
    }

    #[test]
    fn the_three_mistakes_a_generator_makes_are_all_caught() {
        // An unbalanced tag.
        assert!(check("<a><b></a>").is_err());
        assert!(check("<a>").is_err());
        assert!(check("</a>").is_err());
        // An unescaped metacharacter from a path.
        assert!(check("<a>Q&A</a>").is_err());
        assert!(check("<a>&nbsp;</a>").is_err());
        // An unquoted attribute.
        assert!(check("<a x=1/>").is_err());
        assert!(check("<a x=\"1/>").is_err());
    }

    #[test]
    fn a_document_needs_exactly_one_root() {
        assert_eq!(check("").unwrap_err(), "no root element");
        assert!(check("<a/><b/>").unwrap_err().contains("2 root elements"));
    }

    #[test]
    fn a_numeric_entity_is_allowed() {
        check("<a>&#8212;</a>").unwrap();
    }
}
