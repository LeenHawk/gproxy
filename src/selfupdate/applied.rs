//! Staging-channel rollback guard (§19.3): a small on-disk ledger of applied
//! artifact sha256s at `<data_dir>/.update/applied.json`.
//!
//! The `staging` channel decides updates by sha256 (no version ordering), so
//! without a record of what we've installed, an attacker who replays a
//! stale-but-validly-signed manifest could roll the binary *backward* to a
//! superseded build (e.g. one with a known bug). We refuse to install a sha we
//! have already moved past. `releases` is ordered by semver against the
//! compiled-in version and does not use this.
//!
//! Fail-open by design: the ledger lives in the `0700` `.update` dir, so anyone
//! who can tamper with it already holds the update user's privileges (in which
//! case they can replace the binary directly). A read error yields an empty
//! history (the update proceeds); a write error is logged, not fatal.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Keep the most-recent N applied shas — bounds the file. Builds that age out
/// can't be rolled back to anyway, which is acceptable for a dev channel.
const MAX_HISTORY: usize = 32;

#[derive(Default, Serialize, Deserialize)]
struct Ledger {
    /// Lowercase hex sha256s, oldest first, newest last.
    shas: Vec<String>,
}

fn ledger_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".update").join("applied.json")
}

/// Short identifier for an artifact sha in log/error messages.
pub(super) fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// True iff installing `target` would roll back to a build already in `history`
/// that is not the currently-running binary. A new (unseen) sha is allowed —
/// that is the normal staging-forward case; an empty history allows anything.
pub(super) fn is_rollback(history: &[String], current: &str, target: &str) -> bool {
    !target.eq_ignore_ascii_case(current) && history.iter().any(|h| h.eq_ignore_ascii_case(target))
}

/// Load the applied-sha history (empty on any error — fail-open).
pub(super) fn load(data_dir: &Path) -> Vec<String> {
    match std::fs::read(ledger_path(data_dir)) {
        Ok(bytes) => serde_json::from_slice::<Ledger>(&bytes)
            .map(|l| l.shas)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Append `target` to the history, seeding `current` first when the ledger is
/// empty (so a future rollback to the pre-update binary is also caught). Bounded
/// to [`MAX_HISTORY`]; de-duplicates `target` to its newest position.
/// Best-effort: a serialization/write failure is logged, not fatal.
pub(super) fn record(data_dir: &Path, current: &str, target: &str) {
    let mut shas = load(data_dir);
    if shas.is_empty() {
        shas.push(current.to_ascii_lowercase());
    }
    let target = target.to_ascii_lowercase();
    shas.retain(|h| *h != target);
    shas.push(target);
    if shas.len() > MAX_HISTORY {
        shas.drain(0..shas.len() - MAX_HISTORY);
    }

    let body = match serde_json::to_vec_pretty(&Ledger { shas }) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "could not serialize update ledger");
            return;
        }
    };
    let path = ledger_path(data_dir);
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(error = %e, "could not create update ledger dir");
        return;
    }
    if let Err(e) = std::fs::write(&path, body) {
        tracing::warn!(error = %e, "could not persist update ledger");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_detection() {
        let hist = vec!["aa".to_string(), "bb".to_string()];
        // current is bb; going back to the superseded aa is a rollback.
        assert!(is_rollback(&hist, "bb", "aa"));
        // forward to a never-seen sha is allowed.
        assert!(!is_rollback(&hist, "bb", "cc"));
        // re-applying the current sha is not a rollback (and is a no-op anyway).
        assert!(!is_rollback(&hist, "bb", "bb"));
        // case-insensitive on both current and history.
        assert!(!is_rollback(&hist, "BB", "bb"));
        assert!(is_rollback(&hist, "bb", "AA"));
        // empty history allows anything (first-ever apply).
        assert!(!is_rollback(&[], "x", "y"));
    }

    #[test]
    fn record_seeds_current_then_blocks_replay() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".update")).unwrap();

        // First apply: cur0 -> new1. Ledger seeds cur0, then appends new1.
        record(dir.path(), "cur0", "new1");
        assert_eq!(
            load(dir.path()),
            vec!["cur0".to_string(), "new1".to_string()]
        );

        // A replay back to cur0 (now superseded) is caught.
        assert!(is_rollback(&load(dir.path()), "new1", "cur0"));
        // Moving forward to new2 is still fine.
        assert!(!is_rollback(&load(dir.path()), "new1", "new2"));
    }

    #[test]
    fn history_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".update")).unwrap();
        for i in 0..(MAX_HISTORY + 10) {
            record(dir.path(), &format!("sha{i}"), &format!("sha{}", i + 1));
        }
        assert!(load(dir.path()).len() <= MAX_HISTORY);
    }

    #[test]
    fn missing_ledger_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_empty());
    }
}
