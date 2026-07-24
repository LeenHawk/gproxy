use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, TranslateMessage,
};

type ExitSender = Arc<Mutex<Option<oneshot::Sender<()>>>>;

pub(crate) fn start(bind: SocketAddr) -> anyhow::Result<oneshot::Receiver<()>> {
    let (exit_tx, exit_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let url = console_url(bind);

    std::thread::Builder::new()
        .name("gproxy-windows-tray".into())
        .spawn(move || run_tray(url, exit_tx, ready_tx))?;

    ready_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("Windows tray thread stopped during startup"))??;
    Ok(exit_rx)
}

fn run_tray(
    url: String,
    exit_tx: oneshot::Sender<()>,
    ready_tx: std::sync::mpsc::SyncSender<anyhow::Result<()>>,
) {
    if let Err(error) = tray_message_loop(&url, exit_tx, &ready_tx) {
        tracing::error!(%error, "Windows tray stopped unexpectedly");
        let _ = ready_tx.send(Err(error));
    }
}

fn tray_message_loop(
    url: &str,
    exit_tx: oneshot::Sender<()>,
    ready_tx: &std::sync::mpsc::SyncSender<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let open_item = MenuItem::with_id("open-console", "Open Console", true, None);
    let exit_item = MenuItem::with_id("exit-gproxy", "Exit GPROXY", true, None);
    let open_id = open_item.id().clone();
    let exit_id = exit_item.id().clone();
    let menu = Menu::with_items(&[&open_item, &exit_item])?;
    let exit_tx = Arc::new(Mutex::new(Some(exit_tx)));

    let menu_url = url.to_owned();
    let menu_exit = Arc::clone(&exit_tx);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            open_console(&menu_url);
        } else if event.id == exit_id {
            signal_exit(&menu_exit);
        }
    }));

    let click_url = url.to_owned();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            open_console(&click_url);
        }
    }));

    let _tray = TrayIconBuilder::new()
        .with_tooltip("GPROXY is running")
        .with_icon(load_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;
    let _ = ready_tx.send(Ok(()));

    loop {
        let mut message = MSG::default();
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result == 0 {
            break;
        }
        if result == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    signal_exit(&exit_tx);
    Ok(())
}

fn load_icon() -> anyhow::Result<Icon> {
    let image = ico::IconImage::read_png(Cursor::new(include_bytes!(
        "../../docs/public/favicon-96x96.png"
    )))?;
    Ok(Icon::from_rgba(
        image.rgba_data().to_vec(),
        image.width(),
        image.height(),
    )?)
}

fn open_console(url: &str) {
    if let Err(error) = std::process::Command::new("explorer.exe").arg(url).spawn() {
        tracing::warn!(%error, "failed to open Console from tray");
    }
}

fn signal_exit(exit_tx: &ExitSender) {
    if let Some(sender) = exit_tx
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    {
        let _ = sender.send(());
    }
}

fn console_url(bind: SocketAddr) -> String {
    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}/console", SocketAddr::new(ip, bind.port()))
}
