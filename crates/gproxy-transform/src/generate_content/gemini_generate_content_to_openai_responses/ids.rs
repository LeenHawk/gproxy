pub(super) fn call_id(source: Option<String>, index: u32) -> String {
    source.unwrap_or_else(|| format!("call_gemini_{index}"))
}

pub(super) fn item_id(prefix: &str, call_id: &str) -> String {
    format!("{prefix}_{}", stable_suffix(call_id))
}

pub(super) fn reasoning_id(signature: Option<&str>, index: u32) -> String {
    signature.map_or_else(
        || format!("rs_gemini_{index}"),
        |value| item_id("rs", value),
    )
}

fn stable_suffix(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
