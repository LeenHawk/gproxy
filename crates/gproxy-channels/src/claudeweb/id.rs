pub(super) fn fresh(prefix: &str) -> String {
    format!("{prefix}_{}", hex(&random()))
}

pub(super) fn uuid() -> String {
    let mut bytes = random();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    )
}

fn random() -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("operating-system randomness is available");
    bytes
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String succeeds");
    }
    output
}
