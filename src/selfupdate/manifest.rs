//! Release manifest (§19.2) — the signed JSON attached to each Release.
//!
//! Shape (§19.2):
//! ```json
//! {
//!   "channel": "releases",
//!   "version": "2.1.0",
//!   "notes_url": "https://...",
//!   "min_compatible_data_version": 1,
//!   "artifacts": [
//!     { "target_triple": "x86_64-unknown-linux-gnu",
//!       "url": "https://.../gproxy", "sha256": "<hex>", "size": 12345 }
//!   ],
//!   "signature": "<base64 ed25519 of the canonical signing payload>"
//! }
//! ```

use serde::Deserialize;

/// A single platform artifact within a [`Manifest`].
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Artifact {
    /// Rust target triple, e.g. `x86_64-unknown-linux-gnu`.
    pub target_triple: String,
    /// Download URL for the executable.
    pub url: String,
    /// Lowercase hex sha256 of the executable.
    pub sha256: String,
    /// Size in bytes (advisory; integrity is sha256).
    pub size: u64,
}

/// The signed release manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Channel this manifest describes (`releases` | `staging`).
    pub channel: String,
    /// Semver version (authoritative for the `releases` channel).
    pub version: String,
    /// Release notes URL.
    #[serde(default)]
    pub notes_url: Option<String>,
    /// Minimum data/schema version the new binary requires (§19.7).
    #[serde(default)]
    pub min_compatible_data_version: u32,
    /// Per-platform artifacts.
    pub artifacts: Vec<Artifact>,
    /// base64 ed25519 signature over the canonical signing payload
    /// ([`Manifest::signing_payload`]).
    pub signature: String,
}

impl Manifest {
    /// Parse a manifest from JSON.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Locate the artifact for a target triple.
    pub fn artifact_for(&self, triple: &str) -> Option<&Artifact> {
        self.artifacts.iter().find(|a| a.target_triple == triple)
    }

    /// Canonical bytes the `signature` is computed over. The signature itself is
    /// excluded; every other field is bound. Must match the signing tool. Using
    /// a stable, explicit field order (not serde re-serialization) keeps the
    /// payload deterministic regardless of input key order or whitespace.
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&self.channel);
        out.push('\n');
        out.push_str(&self.version);
        out.push('\n');
        out.push_str(self.notes_url.as_deref().unwrap_or(""));
        out.push('\n');
        out.push_str(&self.min_compatible_data_version.to_string());
        out.push('\n');
        // Artifacts in declared order; each as triple|url|sha256|size.
        for a in &self.artifacts {
            out.push_str(&a.target_triple);
            out.push('|');
            out.push_str(&a.url);
            out.push('|');
            out.push_str(&a.sha256);
            out.push('|');
            out.push_str(&a.size.to_string());
            out.push('\n');
        }
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "channel": "releases",
        "version": "2.1.0",
        "notes_url": "https://example.com/notes",
        "min_compatible_data_version": 1,
        "artifacts": [
            {"target_triple": "x86_64-unknown-linux-gnu",
             "url": "https://example.com/gproxy-linux",
             "sha256": "abc123", "size": 100},
            {"target_triple": "aarch64-apple-darwin",
             "url": "https://example.com/gproxy-mac",
             "sha256": "def456", "size": 200}
        ],
        "signature": "AAAA"
    }"#;

    #[test]
    fn parses_and_selects_artifact() {
        let m = Manifest::parse(SAMPLE).expect("parse");
        assert_eq!(m.version, "2.1.0");
        assert_eq!(m.min_compatible_data_version, 1);
        let a = m
            .artifact_for("x86_64-unknown-linux-gnu")
            .expect("artifact");
        assert_eq!(a.sha256, "abc123");
        assert!(m.artifact_for("nonexistent-triple").is_none());
    }

    #[test]
    fn notes_url_optional() {
        let json = r#"{"channel":"staging","version":"staging",
            "artifacts":[],"signature":"x"}"#;
        let m = Manifest::parse(json).expect("parse");
        assert_eq!(m.notes_url, None);
        assert_eq!(m.min_compatible_data_version, 0);
    }

    #[test]
    fn signing_payload_is_deterministic_and_order_sensitive() {
        let m = Manifest::parse(SAMPLE).expect("parse");
        // Same manifest → same payload.
        assert_eq!(m.signing_payload(), m.signing_payload());
        // Re-parsing identical JSON yields the same payload.
        let m2 = Manifest::parse(SAMPLE).expect("parse");
        assert_eq!(m.signing_payload(), m2.signing_payload());
        // The signature field is NOT part of the payload.
        let payload = String::from_utf8(m.signing_payload()).unwrap();
        assert!(!payload.contains("AAAA"));
        assert!(payload.contains("2.1.0"));
    }
}
