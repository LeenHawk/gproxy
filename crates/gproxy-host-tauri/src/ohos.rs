//! The Ability owns OHOS background tasks. N-API dispatches requests onto its
//! ArkTS thread; no file polling and no independent proxy process.
use std::sync::{Arc, Mutex};

use napi_derive_ohos::napi;
use napi_ohos::{
    Env,
    bindgen_prelude::{Function, Promise},
    threadsafe_function::ThreadsafeFunction,
};
use serde::{Deserialize, Serialize};

use crate::{IpcError, IpcResult};

type Handler = ThreadsafeFunction<String, Promise<String>, String, napi_ohos::Status, false>;
static HANDLER: Mutex<Option<Arc<Handler>>> = Mutex::new(None);

#[napi]
pub fn register_gproxy_background(
    handler: Function<'_, String, Promise<String>>,
) -> napi_ohos::Result<()> {
    let handler = handler
        .build_threadsafe_function::<String>()
        .callee_handled::<false>()
        .build()?;
    *HANDLER
        .lock()
        .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))? =
        Some(Arc::new(handler));
    Ok(())
}

#[napi]
pub fn gproxy_engine_running() -> bool {
    crate::engine::started().is_some_and(|engine| *engine.running().borrow())
}

#[napi]
pub fn gproxy_shutdown() {
    crate::engine::shutdown();
    // Like Android, destruction is a cold stop: the process-global engine
    // cannot be reused after shutdown by another Ability in this process.
    std::process::exit(0);
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundStatus {
    pub supported: bool,
    pub device_type: String,
    pub enabled: bool,
    pub active: bool,
    pub cancelled: bool,
    pub error: Option<String>,
    pub auto_start: Option<bool>,
    pub startup_error: Option<String>,
    pub start_minimized: bool,
    pub can_minimize: bool,
    pub can_tray: bool,
    pub tray: bool,
    pub tray_ready: bool,
    pub close_to_tray: bool,
    pub tray_error: Option<String>,
}

pub async fn request(command: &str) -> IpcResult<BackgroundStatus> {
    let handler = HANDLER
        .lock()
        .map_err(IpcError::internal)?
        .clone()
        .ok_or_else(|| IpcError::internal("OpenHarmony background task bridge is not ready"))?;
    let result = handler
        .call_async(command.to_owned())
        .await
        .map_err(IpcError::internal)?
        .await
        .map_err(IpcError::internal)?;
    serde_json::from_str(&result).map_err(IpcError::internal)
}

pub fn restore() {
    tauri::async_runtime::spawn(async {
        if let Err(error) = request("restore").await {
            tracing::warn!(message = %error.message, "could not restore the OpenHarmony background task");
        }
        if let Some(engine) = crate::engine::started() {
            let mut running = engine.running();
            let _ = running.changed().await;
            let _ = request("refresh-tray").await;
        }
    });
}

/// Resolve the API on the running OS, so an API 20 build can query it on API
/// 21+ without replacing the pinned Tauri toolchain. Called on the ArkTS thread.
#[napi]
pub fn gproxy_auto_startup_status(
    env: Env,
) -> napi_ohos::Result<napi_ohos::bindgen_prelude::PromiseRaw<'static, bool>> {
    env.load("@ohos.app.ability.autoStartupManager")?
        .call("getAutoStartupStatusForSelf", ())
}

#[napi]
pub fn gproxy_language() -> String {
    crate::engine::started()
        .and_then(|engine| crate::setup::read_choices(engine.data_dir()).ok())
        .map(|choices| choices.preferences.language)
        .unwrap_or_else(|| "en".into())
}
