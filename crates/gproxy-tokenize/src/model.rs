const O200K: &[&str] = &["gpt-4o", "gpt-4.1", "gpt-5", "o1", "o3", "o4"];
const CL100K: &[&str] = &["gpt-3.5", "gpt-4"];

pub fn is_gpt_family(model: &str) -> bool {
    O200K
        .iter()
        .chain(CL100K)
        .any(|prefix| model.starts_with(prefix))
}

#[cfg(feature = "tiktoken")]
pub(crate) fn gpt_encoding(model: &str) -> Option<(&'static tiktoken_rs::CoreBPE, &'static str)> {
    if O200K.iter().any(|prefix| model.starts_with(prefix)) {
        Some((tiktoken_rs::o200k_base_singleton(), "o200k_base"))
    } else if CL100K.iter().any(|prefix| model.starts_with(prefix)) {
        Some((tiktoken_rs::cl100k_base_singleton(), "cl100k_base"))
    } else {
        None
    }
}

#[cfg(feature = "hf-registry")]
pub(crate) fn select_vocab(map: Option<&serde_json::Value>, model: &str) -> Option<String> {
    let mut best: Option<(&str, usize, &str)> = None;
    for (pattern, value) in map?.as_object()? {
        let Some(vocab) = value.as_str() else {
            continue;
        };
        if !glob_matches(pattern, model) {
            continue;
        }
        let specificity = pattern.bytes().filter(|byte| *byte != b'*').count();
        if best.is_none_or(|(best_pattern, best_specificity, _)| {
            specificity > best_specificity
                || (specificity == best_specificity && pattern.as_str() < best_pattern)
        }) {
            best = Some((pattern, specificity, vocab));
        }
    }
    best.map(|(_, _, vocab)| vocab.to_owned())
}

#[cfg(feature = "hf-registry")]
fn glob_matches(pattern: &str, value: &str) -> bool {
    let (pattern, value) = (pattern.as_bytes(), value.as_bytes());
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry_value) = (None, 0);
    while value_index < value.len() {
        if pattern.get(pattern_index) == Some(&value[value_index]) {
            pattern_index += 1;
            value_index += 1;
        } else if pattern.get(pattern_index) == Some(&b'*') {
            star = Some(pattern_index);
            pattern_index += 1;
            retry_value = value_index;
        } else if let Some(star_index) = star {
            retry_value += 1;
            value_index = retry_value;
            pattern_index = star_index + 1;
        } else {
            return false;
        }
    }
    while pattern.get(pattern_index) == Some(&b'*') {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}
