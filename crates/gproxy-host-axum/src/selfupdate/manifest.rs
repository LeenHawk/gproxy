use serde::Deserialize;

use super::{Error, Result};

#[derive(Clone, Deserialize)]
pub(super) struct Artifact {
    pub target_triple: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Deserialize)]
pub(super) struct Manifest {
    pub channel: String,
    pub version: String,
    pub notes_url: Option<String>,
    pub min_compatible_data_version: u32,
    pub artifacts: Vec<Artifact>,
    pub signature: String,
}

impl Manifest {
    pub(super) fn parse_verified(bytes: &[u8]) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|_| Error::Manifest)?;
        crate::signature::verify_detached(&manifest.signing_payload(), &manifest.signature)
            .map_err(|_| Error::Signature)?;
        Ok(manifest)
    }

    #[cfg(test)]
    pub(super) fn parse_verified_with_key(bytes: &[u8], key: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|_| Error::Manifest)?;
        crate::signature::verify_detached_with_key(
            &manifest.signing_payload(),
            &manifest.signature,
            Some(key),
        )
        .map_err(|_| Error::Signature)?;
        Ok(manifest)
    }

    pub(super) fn artifact(&self, target: &str) -> Result<&Artifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target_triple == target)
            .ok_or(Error::Artifact)
    }

    fn signing_payload(&self) -> Vec<u8> {
        let mut output = format!(
            "{}\n{}\n{}\n{}\n",
            self.channel,
            self.version,
            self.notes_url.as_deref().unwrap_or(""),
            self.min_compatible_data_version
        );
        for artifact in &self.artifacts {
            output.push_str(&format!(
                "{}|{}|{}|{}\n",
                artifact.target_triple, artifact.url, artifact.sha256, artifact.size
            ));
        }
        output.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use ed25519_dalek::{Signer as _, SigningKey};

    #[test]
    fn tampered_manifest_is_rejected() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let public_key =
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
        let payload = "releases\n3.1.0\n\n1\nx86_64-unknown-linux-gnu|https://example.test/gproxy.zip|abcd|4\n";
        let signature = base64::engine::general_purpose::STANDARD
            .encode(key.sign(payload.as_bytes()).to_bytes());
        let json = format!(
            r#"{{"channel":"releases","version":"3.1.0","notes_url":null,"min_compatible_data_version":1,"artifacts":[{{"target_triple":"x86_64-unknown-linux-gnu","url":"https://example.test/gproxy.zip","sha256":"abcd","size":4}}],"signature":"{signature}"}}"#
        );
        assert!(super::Manifest::parse_verified_with_key(json.as_bytes(), &public_key).is_ok());
        let tampered = json.replace("3.1.0", "3.2.0");
        assert!(
            super::Manifest::parse_verified_with_key(tampered.as_bytes(), &public_key).is_err()
        );
    }
}
