//! Lowercase hexadecimal, without a dependency for twenty lines of it.

/// Lowercase hex of `bytes`.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// Decode hex in either case. None for an odd length or any non-hex character;
/// there is no partial decode, because a half-read key or digest that still
/// compares equal to something is worse than a rejected one.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_round_trips() {
        let bytes: Vec<u8> = (0..=255_u8).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
        assert_eq!(encode(&[0, 15, 16, 255]), "000f10ff");
    }

    #[test]
    fn either_case_decodes_to_the_same_bytes() {
        assert_eq!(decode("DEADbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn malformed_input_decodes_to_nothing() {
        assert_eq!(decode("abc"), None);
        assert_eq!(decode("zz"), None);
        assert_eq!(decode("ab cd"), None);
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }
}
