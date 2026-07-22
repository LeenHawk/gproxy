//! Incremental UTF-8 decoding for byte streams.
//!
//! Network chunks may split a multi-byte UTF-8 sequence across chunk
//! boundaries. Decoding each chunk independently (e.g. with
//! `String::from_utf8_lossy`) turns the split character into U+FFFD
//! replacement characters. This decoder holds back an incomplete trailing
//! sequence until the next chunk completes it.

/// Streaming UTF-8 decoder: feed byte chunks, get valid `&str` pushed into a
/// `String`. Truly invalid bytes are replaced with U+FFFD; an *incomplete*
/// trailing sequence is buffered until more bytes arrive (or [`Self::flush`]).
#[derive(Debug, Default)]
pub struct Utf8StreamDecoder {
    /// Incomplete trailing UTF-8 sequence (at most 3 bytes) held back until
    /// the next chunk.
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    /// Decode `chunk` (prefixed by any held-back bytes) into `out`.
    pub fn decode_into(&mut self, chunk: &[u8], out: &mut String) {
        let owned;
        let mut rest: &[u8] = if self.pending.is_empty() {
            chunk
        } else {
            self.pending.extend_from_slice(chunk);
            owned = std::mem::take(&mut self.pending);
            &owned
        };
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    return;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    out.push_str(std::str::from_utf8(valid).expect("valid prefix"));
                    match e.error_len() {
                        // Genuinely invalid bytes: replace and continue.
                        Some(n) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[n..];
                        }
                        // Incomplete trailing sequence: hold back for the
                        // next chunk.
                        None => {
                            self.pending = after.to_vec();
                            return;
                        }
                    }
                }
            }
        }
    }

    /// End of stream: replace any held-back incomplete sequence with U+FFFD.
    pub fn flush(&mut self, out: &mut String) {
        if !self.pending.is_empty() {
            self.pending.clear();
            out.push(char::REPLACEMENT_CHARACTER);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multibyte_char_split_across_chunks() {
        let bytes = "汉字🚀".as_bytes(); // 3+3+4 bytes
        let mut d = Utf8StreamDecoder::default();
        let mut out = String::new();
        // Split inside every character.
        for b in bytes {
            d.decode_into(std::slice::from_ref(b), &mut out);
        }
        d.flush(&mut out);
        assert_eq!(out, "汉字🚀");
    }

    #[test]
    fn invalid_bytes_replaced_incomplete_tail_flushed() {
        let mut d = Utf8StreamDecoder::default();
        let mut out = String::new();
        d.decode_into(b"a\xffb\xe6\xb1", &mut out); // 0xff invalid, e6 b1 incomplete
        assert_eq!(out, "a\u{fffd}b");
        d.flush(&mut out);
        assert_eq!(out, "a\u{fffd}b\u{fffd}");
    }
}
