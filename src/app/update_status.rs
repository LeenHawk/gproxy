//! §19.10 self-update status (admin API). Lives in `app/` (not `admin/`) because
//! `AppState` carries it and `AppState` is wasm-compiled, while `admin` is native-only.

/// In-process self-update status state machine. `serde(tag="state")` →
/// wire shape `{"state":"staged","version":"1.2.3"}`.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Downloading,
    Staged {
        version: String,
    },
    Failed {
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::UpdateStatus;

    #[test]
    fn update_status_serde_shapes() {
        assert_eq!(
            serde_json::to_value(UpdateStatus::Idle).unwrap(),
            serde_json::json!({"state": "idle"})
        );
        assert_eq!(
            serde_json::to_value(UpdateStatus::Staged {
                version: "1.2.3".to_string()
            })
            .unwrap(),
            serde_json::json!({"state": "staged", "version": "1.2.3"})
        );
        assert_eq!(
            serde_json::to_value(UpdateStatus::Failed {
                error: "x".to_string()
            })
            .unwrap(),
            serde_json::json!({"state": "failed", "error": "x"})
        );
    }
}
