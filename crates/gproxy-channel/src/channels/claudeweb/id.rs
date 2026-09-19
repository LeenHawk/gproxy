//! Random identifiers claude.ai lets the client choose: conversation and
//! turn-message UUIDs, synthesized message ids (v3 `claudeweb/id.rs`).

use crate::channel::ChannelError;

/// `{prefix}_{32 hex chars}`, e.g. a Claude Messages `msg_` id.
pub(super) fn fresh(prefix: &str) -> Result<String, ChannelError> {
    Ok(format!("{prefix}_{}", hex(&random()?)))
}

/// A random version-4 UUID in its canonical lowercase form.
pub(super) fn uuid() -> Result<String, ChannelError> {
    let mut bytes = random()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    ))
}

fn random() -> Result<[u8; 16], ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    Ok(bytes)
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String succeeds");
    }
    output
}
