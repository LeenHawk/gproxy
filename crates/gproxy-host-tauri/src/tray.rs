use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[derive(Default)]
struct Monitor(std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>);

pub fn remove<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(monitor) = app.try_state::<Monitor>()
        && let Some(task) = monitor.0.lock().unwrap().take()
    {
        task.abort();
    }
    app.remove_tray_by_id("gproxy");
}

fn labels(language: &str) -> [&'static str; 4] {
    match language {
        "zh-CN" => ["打开 GPROXY", "退出 GPROXY", "代理正在运行", "代理已停止"],
        "zh-TW" => ["開啟 GPROXY", "結束 GPROXY", "代理正在執行", "代理已停止"],
        _ => [
            "Open GPROXY",
            "Quit GPROXY",
            "Proxy is running",
            "Proxy has stopped",
        ],
    }
}

pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>, language: &str) -> tauri::Result<()> {
    let [open, quit, running, stopped] = labels(language);
    let engine = crate::engine::started();
    let is_running = engine.is_some_and(|engine| *engine.running().borrow());
    let status = MenuItem::with_id(
        app,
        "status",
        if is_running { running } else { stopped },
        false,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, "show", open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&status, &separator, &show, &quit])?;
    if let Some(tray) = app.tray_by_id("gproxy") {
        tray.set_menu(Some(menu))?;
    } else {
        let mut builder = TrayIconBuilder::with_id("gproxy")
            .tooltip("GPROXY")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id.as_ref() {
                "show" => show_window(app),
                "quit" => {
                    crate::engine::shutdown();
                    app.exit(0);
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_window(tray.app_handle());
                }
            });
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        builder.build(app)?;
    }
    app.manage(Monitor::default());
    let monitor = app.state::<Monitor>();
    let mut task = monitor.0.lock().unwrap();
    if let Some(previous) = task.take() {
        previous.abort();
    }
    if let Some(engine) = engine {
        let mut state = engine.running();
        // The listener owns this signal; stopping or failing updates the menu.
        *task = Some(tauri::async_runtime::spawn(async move {
            let _ = state.changed().await;
            let _ = status.set_text(stopped);
        }));
    }
    Ok(())
}

pub(crate) fn show_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
