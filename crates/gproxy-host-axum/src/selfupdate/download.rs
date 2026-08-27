use sha2::{Digest as _, Sha256};

use super::manifest::{Artifact, Manifest};
use super::{Error, Result};

pub(super) async fn manifest(client: &wreq::Client, url: &str) -> Result<Manifest> {
    let response = client.get(url).send().await.map_err(|_| Error::Download)?;
    if !response.status().is_success() {
        return Err(Error::Download);
    }
    let bytes = response.bytes().await.map_err(|_| Error::Download)?;
    Manifest::parse_verified(&bytes)
}

pub(super) async fn artifact(client: &wreq::Client, artifact: &Artifact) -> Result<Vec<u8>> {
    let response = client
        .get(&artifact.url)
        .send()
        .await
        .map_err(|_| Error::Download)?;
    if !response.status().is_success() {
        return Err(Error::Download);
    }
    let bytes = response.bytes().await.map_err(|_| Error::Download)?;
    if bytes.len() as u64 != artifact.size {
        return Err(Error::Integrity);
    }
    let actual = hex(&Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(artifact.sha256.trim()) {
        return Err(Error::Integrity);
    }
    Ok(bytes.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
