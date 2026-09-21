//! The signed manifest: the one document this whole module trusts.
//!
//! # The signing payload is not the JSON
//!
//! The signature covers a **rendered** byte string, not the document it came
//! in. Four lines of scalars, then one line per artifact:
//!
//! ```text
//! release\n
//! 4.1.0\n
//! https://github.com/LeenHawk/gproxy/releases/tag/v4.1.0\n
//! 4\n
//! x86_64-unknown-linux-gnu|https://…/gproxy-x86_64.zip|<sha256>|18234112\n
//! aarch64-apple-darwin|https://…/gproxy-aarch64.zip|<sha256>|17772336\n
//! ```
//!
//! Signing the JSON text instead would mean either signing it byte for byte —
//! and then a proxy that re-serializes it, or `jq` in a release script,
//! invalidates a valid manifest — or canonicalizing it first, which is a
//! specification of its own to get wrong. This form has one producer
//! (`scripts/build-update-manifest.sh`, which writes the same lines with
//! `printf`) and one consumer ([`Manifest::signing_payload`]), and every field
//! that decides what gets installed is in it: the channel, the version, the
//! data-version floor, and every artifact's URL, hash and size.
//!
//! A field **not** in the payload is not trusted, which is why there is only
//! one — `notes_url` is in it too, so even the link cannot be swapped.
//!
//! Ported from v3 unchanged, including the byte layout: a v3 manifest verifies
//! here, and a manifest this build accepts verifies there.

use serde::Deserialize;

use super::config::UpdateError;
use super::signature;

/// One downloadable build.
#[derive(Clone, Debug, Deserialize)]
pub(super) struct Artifact {
    /// The key [`super::version::target`] looks itself up by, e.g.
    /// `x86_64-unknown-linux-gnu`.
    pub target_triple: String,
    pub url: String,
    /// Lowercase hex, checked before the bytes are unpacked.
    pub sha256: String,
    /// Checked too, and first: a length mismatch is cheaper to notice than a
    /// hash mismatch and catches a truncated download without hashing 18 MB.
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Manifest {
    pub channel: String,
    pub version: String,
    pub notes_url: Option<String>,
    /// The oldest data layout this release can open. Compared against
    /// [`super::version::DATA_VERSION`] before anything is downloaded.
    pub min_compatible_data_version: u32,
    pub artifacts: Vec<Artifact>,
    pub signature: String,
}

impl Manifest {
    /// Parse and verify, in that order and never only the first.
    ///
    /// There is no `parse` without the verification, deliberately: a
    /// `Manifest` value in this module is by construction one whose signature
    /// checked out, so no call site can forget. `key` is `None` for a build
    /// compiled without `GPROXY_UPDATE_PUBKEY`, which then refuses every
    /// manifest rather than accepting an unsigned one.
    pub(super) fn parse_verified(bytes: &[u8], key: Option<&str>) -> Result<Self, UpdateError> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|_| UpdateError::Manifest)?;
        signature::verify_detached_with_key(&manifest.signing_payload(), &manifest.signature, key)?;
        Ok(manifest)
    }

    pub(super) fn artifact(&self, target: &str) -> Result<&Artifact, UpdateError> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target_triple == target)
            .ok_or_else(|| UpdateError::Artifact(target.to_owned()))
    }

    pub(super) fn signing_payload(&self) -> Vec<u8> {
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
pub(super) mod fixture {
    //! Signing a manifest, for the tests that need a real one.
    //!
    //! This is the half of `scripts/build-update-manifest.sh` that matters —
    //! the same payload lines, the same detached signature — so a test that
    //! passes here is a test against the wire format the release pipeline
    //! writes.

    use base64::Engine as _;
    use ed25519_dalek::{Signer as _, SigningKey};

    pub(crate) fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub(crate) fn public_key(key: &SigningKey) -> String {
        base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes())
    }

    /// One artifact, as the script renders it into both the payload and the
    /// JSON.
    pub(crate) struct Entry {
        pub target: String,
        pub url: String,
        pub sha256: String,
        pub size: u64,
    }

    /// Render and sign a manifest. Returns the JSON document.
    pub(crate) fn manifest(
        key: &SigningKey,
        channel: &str,
        version: &str,
        notes_url: Option<&str>,
        minimum: u32,
        artifacts: &[Entry],
    ) -> String {
        let mut payload = format!(
            "{channel}\n{version}\n{}\n{minimum}\n",
            notes_url.unwrap_or("")
        );
        for entry in artifacts {
            payload.push_str(&format!(
                "{}|{}|{}|{}\n",
                entry.target, entry.url, entry.sha256, entry.size
            ));
        }
        let signature = base64::engine::general_purpose::STANDARD
            .encode(key.sign(payload.as_bytes()).to_bytes());
        let rendered: Vec<String> = artifacts
            .iter()
            .map(|entry| {
                format!(
                    r#"{{"target_triple":"{}","url":"{}","sha256":"{}","size":{}}}"#,
                    entry.target, entry.url, entry.sha256, entry.size
                )
            })
            .collect();
        let notes = match notes_url {
            Some(url) => format!(r#""{url}""#),
            None => "null".to_owned(),
        };
        format!(
            r#"{{"channel":"{channel}","version":"{version}","notes_url":{notes},"min_compatible_data_version":{minimum},"artifacts":[{}],"signature":"{signature}"}}"#,
            rendered.join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> fixture::Entry {
        fixture::Entry {
            target: "x86_64-unknown-linux-gnu".into(),
            url: "https://example.test/gproxy.zip".into(),
            sha256: "abcd".into(),
            size: 4,
        }
    }

    #[test]
    fn a_signed_manifest_parses_and_a_tampered_one_does_not() {
        let key = fixture::signing_key(9);
        let public = fixture::public_key(&key);
        let json = fixture::manifest(&key, "release", "4.1.0", None, 4, &[entry()]);

        let manifest = Manifest::parse_verified(json.as_bytes(), Some(&public)).unwrap();
        assert_eq!(manifest.version, "4.1.0");
        assert_eq!(manifest.min_compatible_data_version, 4);

        // Every field in the payload is a field an attacker cannot move.
        for (from, to) in [
            ("4.1.0", "9.9.9"),
            ("release", "beta"),
            ("https://example.test/gproxy.zip", "https://evil.test/x.zip"),
            (r#""sha256":"abcd""#, r#""sha256":"dcba""#),
            (r#""size":4"#, r#""size":5"#),
            (
                r#""min_compatible_data_version":4"#,
                r#""min_compatible_data_version":1"#,
            ),
        ] {
            let tampered = json.replace(from, to);
            assert_ne!(tampered, json, "the test's own replacement did nothing");
            let error = Manifest::parse_verified(tampered.as_bytes(), Some(&public))
                .expect_err(&format!("accepted a manifest with {from} changed to {to}"));
            assert!(matches!(error, UpdateError::Signature), "{error}");
        }
    }

    /// Even the link a human is meant to read before installing is signed, so
    /// a manifest host cannot point the operator's "what changed?" at a page
    /// it wrote.
    #[test]
    fn the_notes_url_is_covered_too() {
        let key = fixture::signing_key(3);
        let public = fixture::public_key(&key);
        let json = fixture::manifest(
            &key,
            "release",
            "4.1.0",
            Some("https://example.test/notes"),
            4,
            &[entry()],
        );
        assert!(Manifest::parse_verified(json.as_bytes(), Some(&public)).is_ok());
        let tampered = json.replace("https://example.test/notes", "https://evil.test/notes");
        assert!(Manifest::parse_verified(tampered.as_bytes(), Some(&public)).is_err());
    }

    #[test]
    fn an_unparseable_document_is_a_manifest_failure_not_a_signature_one() {
        let error = Manifest::parse_verified(b"{}", Some("AA==")).unwrap_err();
        assert!(matches!(error, UpdateError::Manifest), "{error}");
    }

    #[test]
    fn a_missing_target_names_the_target_it_looked_for() {
        let key = fixture::signing_key(4);
        let public = fixture::public_key(&key);
        let json = fixture::manifest(&key, "release", "4.1.0", None, 4, &[entry()]);
        let manifest = Manifest::parse_verified(json.as_bytes(), Some(&public)).unwrap();
        assert!(manifest.artifact("x86_64-unknown-linux-gnu").is_ok());
        let error = manifest.artifact("sparc64-unknown-netbsd").unwrap_err();
        assert!(
            error.to_string().contains("sparc64-unknown-netbsd"),
            "{error}"
        );
    }
}
