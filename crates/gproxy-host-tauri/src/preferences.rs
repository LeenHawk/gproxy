//! Application preferences belong to the local shell, not the gateway database.
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{IpcError, IpcResult, setup::Setup};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub auto_start: bool,
    pub tray: bool,
    pub close_to_tray: bool,
    pub start_hidden: bool,
    pub language: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            auto_start: false,
            tray: cfg!(desktop),
            close_to_tray: true,
            start_hidden: true,
            language: "en".into(),
        }
    }
}

impl Preferences {
    pub fn normalized(mut self) -> Self {
        self.tray &= cfg!(desktop);
        self.close_to_tray &= self.tray;
        self
    }

    pub fn hides_on_launch(&self, autostart: bool, completed: bool, tray_ready: bool) -> bool {
        autostart && completed && self.auto_start && self.start_hidden && self.tray && tray_ready
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    #[serde(flatten)]
    pub preferences: Preferences,
    pub desktop: bool,
    pub can_auto_start: bool,
    pub startup_error: Option<String>,
    pub tray_error: Option<String>,
    pub platform: &'static str,
    #[cfg(target_env = "ohos")]
    pub ohos: crate::ohos::BackgroundStatus,
}

#[derive(Default)]
pub struct RuntimeStatus(pub std::sync::Mutex<Option<String>>);

pub async fn status<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    setup: &Setup,
) -> IpcResult<Status> {
    let preferences = setup.choices().map_err(IpcError::internal)?.preferences;
    #[cfg(desktop)]
    let (preferences, startup_error) = {
        let mut preferences = preferences;
        let error = match tauri::async_runtime::spawn_blocking(crate::startup::get)
            .await
            .map_err(IpcError::internal)?
        {
            Ok(enabled) => {
                preferences.auto_start = enabled;
                None
            }
            Err(error) => Some(error),
        };
        (preferences, error)
    };
    #[cfg(not(desktop))]
    let startup_error = None;
    let tray_error = app
        .try_state::<RuntimeStatus>()
        .and_then(|state| state.0.lock().ok().and_then(|value| value.clone()));
    Ok(Status {
        preferences,
        desktop: cfg!(desktop),
        can_auto_start: !cfg!(target_env = "ohos"),
        startup_error,
        tray_error,
        platform: if cfg!(target_env = "ohos") {
            "ohos"
        } else if cfg!(target_os = "android") {
            "android"
        } else {
            "desktop"
        },
        #[cfg(target_env = "ohos")]
        ohos: crate::ohos::request("status").await?,
    })
}

pub async fn save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    setup: &Setup,
    preferences: Preferences,
) -> IpcResult<Status> {
    let _gate = setup.gate.lock().await;
    let mut choices = setup.choices().map_err(IpcError::internal)?;
    if !choices.completed {
        return Err(gproxy_app::AppError::invalid("finish the initial setup first").into());
    }
    if cfg!(target_env = "ohos") && preferences.auto_start {
        return Err(gproxy_app::AppError::invalid(
            "OpenHarmony does not allow this application to register login startup",
        )
        .into());
    }
    let previous = choices.preferences.clone();
    #[cfg(desktop)]
    let previous_startup = tauri::async_runtime::spawn_blocking(crate::startup::get)
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::internal)?;
    #[cfg(not(desktop))]
    let previous_startup = previous.auto_start;
    #[cfg(desktop)]
    let had_tray = app.tray_by_id("gproxy").is_some();
    choices.preferences = preferences.normalized();
    let enabled = choices.preferences.auto_start;
    tauri::async_runtime::spawn_blocking(move || crate::startup::set(enabled))
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::internal)?;
    let result = (|| {
        #[cfg(desktop)]
        if choices.preferences.tray {
            crate::tray::install(app, &choices.preferences.language).map_err(IpcError::internal)?;
        }
        crate::setup::write_choices(&setup.root, &choices).map_err(IpcError::internal)
    })();
    if let Err(mut error) = result {
        let rollback =
            tauri::async_runtime::spawn_blocking(move || crate::startup::set(previous_startup))
                .await;
        if !matches!(rollback, Ok(Ok(()))) {
            error.message.push_str(&format!(
                "; could not restore launch-at-login registration: {rollback:?}"
            ));
        }
        #[cfg(desktop)]
        if had_tray {
            if let Err(rollback) = crate::tray::install(app, &previous.language) {
                error
                    .message
                    .push_str(&format!("; could not restore the tray: {rollback}"));
            }
        } else {
            crate::tray::remove(app);
        }
        return Err(error);
    }
    #[cfg(desktop)]
    {
        if !choices.preferences.tray {
            crate::tray::show_window(app);
            crate::tray::remove(app);
        }
        if let Some(state) = app.try_state::<RuntimeStatus>() {
            *state.0.lock().map_err(IpcError::internal)? = None;
        }
    }
    status(app, setup).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn existing_setup_files_keep_the_flat_wire_contract() {
        let choices: crate::setup::Choices = serde_json::from_value(serde_json::json!({
            "dataDir": "/old/instance", "host": "127.0.0.1", "port": 8787,
            "adminUser": "admin", "autoStart": true, "tray": false, "completed": true
        }))
        .unwrap();
        assert!(choices.preferences.auto_start);
        assert!(!choices.preferences.tray);
        assert!(choices.preferences.close_to_tray);
        assert!(choices.preferences.start_hidden);
        let wire = serde_json::to_value(choices).unwrap();
        assert_eq!(wire["autoStart"], true);
        assert_eq!(wire["tray"], false);
        assert!(wire.get("preferences").is_none());
    }
}
