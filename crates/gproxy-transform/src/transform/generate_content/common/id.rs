pub(in crate::transform::generate_content) fn response_call_id(original: &str) -> String {
    prefixed_response_id(original, "call_")
}

pub(in crate::transform::generate_content) fn response_function_call_item_id(
    original: &str,
) -> String {
    prefixed_response_id(original, "fc_")
}

pub(in crate::transform::generate_content) fn indexed_response_call_id(index: u32) -> String {
    format!("call_{index}")
}

pub(in crate::transform::generate_content) fn indexed_response_function_call_item_id(
    index: u32,
) -> String {
    format!("fc_{index}")
}

pub(in crate::transform::generate_content) fn fallback_response_call_id(
    index: u32,
    item_id: Option<&str>,
) -> String {
    if let Some(item_id) = item_id {
        if item_id.starts_with("call_") || item_id.starts_with("toolu_") {
            return item_id.to_owned();
        }

        for prefix in ["fc_", "ctc_"] {
            if let Some(suffix) = item_id.strip_prefix(prefix)
                && !suffix.is_empty()
            {
                return format!("call_{suffix}");
            }
        }
    }

    indexed_response_call_id(index)
}

fn prefixed_response_id(original: &str, prefix: &str) -> String {
    let bare_prefix = prefix.trim_end_matches('_');
    if original.starts_with(bare_prefix) {
        original.to_owned()
    } else {
        format!("{prefix}{}", stable_response_id_suffix(original))
    }
}

fn stable_response_id_suffix(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
