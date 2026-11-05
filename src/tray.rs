//! Native Windows tray: a blocking message loop, no GUI framework or polling.
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use tokio::sync::Notify;
use windows_sys::Win32::{
    Foundation::*,
    System::{
        Console::{AttachConsole, ATTACH_PARENT_PROCESS},
        LibraryLoader::GetModuleHandleW,
    },
    UI::{Shell::*, WindowsAndMessaging::*},
};

use crate::Config;

const TRAY_MESSAGE: u32 = WM_APP + 1;
const OPEN: usize = 1;
const CONNECT: usize = 2;
const EXIT: usize = 3;
const STARTUP: usize = 4;
const CONNECTION_STATUS: u32 = WM_APP + 2;

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

pub fn attach_console() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn show_error(message: &str) {
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            wide(message).as_ptr(),
            wide("Liquid Deck").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn panel_url(config: &Config, phone: bool) -> String {
    let host = if phone || config.bind.ip().is_unspecified() {
        "127.0.0.1".into()
    } else {
        match config.bind.ip() {
            std::net::IpAddr::V6(ip) => format!("[{ip}]"),
            ip => ip.to_string(),
        }
    };
    let mut url = format!("http://{host}:{}/", config.bind.port());
    if let Some(token) = &config.token {
        url.push_str("?token=");
        for byte in token.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                url.push(byte as char);
            } else {
                use std::fmt::Write;
                let _ = write!(url, "%{byte:02X}");
            }
        }
    }
    url
}

struct Context {
    menu: HMENU,
    icon: NOTIFYICONDATAW,
    taskbar_created: u32,
    url: String,
    runtime: tokio::runtime::Handle,
    phone: PhoneConnection,
}

#[derive(Clone)]
struct PhoneConnection {
    url: String,
    port: u16,
    busy: Arc<AtomicBool>,
    status: Arc<Mutex<&'static str>>,
    window: usize,
}

impl PhoneConnection {
    fn publish(&self, status: &'static str) {
        if let Ok(mut current) = self.status.lock() {
            *current = status;
        }
        unsafe {
            PostMessageW(self.window as HWND, CONNECTION_STATUS, 0, 0);
        }
    }

    fn connect(&self, runtime: &tokio::runtime::Handle, automatic: bool) {
        if self.busy.swap(true, Ordering::Relaxed) {
            return;
        }
        let phone = self.clone();
        runtime.spawn(async move {
            phone.publish("Liquid Deck — Note 4 bağlanıyor");
            let delays: &[u64] = if automatic { &[0, 5, 10, 20, 40] } else { &[0] };
            let mut last_error = String::new();
            let mut connected = false;
            for delay in delays {
                if *delay > 0 {
                    tokio::time::sleep(Duration::from_secs(*delay)).await;
                }
                match tokio::time::timeout(
                    Duration::from_secs(20),
                    connect_phone(phone.port, &phone.url),
                )
                .await
                {
                    Ok(Ok(())) => {
                        connected = true;
                        break;
                    }
                    Ok(Err(error)) => last_error = error,
                    Err(_) => last_error = "Note 4 bağlantısı zaman aşımına uğradı".into(),
                }
            }
            phone.busy.store(false, Ordering::Relaxed);
            phone.publish(if connected {
                "Liquid Deck — Note 4 bağlı"
            } else {
                "Liquid Deck — Note 4 çevrimdışı; menüden yeniden deneyin"
            });
            if !connected && !automatic {
                let _ = tokio::task::spawn_blocking(move || show_error(&last_error)).await;
            }
        });
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let context = (GetWindowLongPtrW(window, GWLP_USERDATA) as *const Context).as_ref();
    if let Some(context) = context {
        if message == context.taskbar_created {
            Shell_NotifyIconW(NIM_ADD, &context.icon);
            PostMessageW(window, CONNECTION_STATUS, 0, 0);
            return 0;
        }
        match message {
            CONNECTION_STATUS => {
                let status = context
                    .phone
                    .status
                    .lock()
                    .map(|s| *s)
                    .unwrap_or("Liquid Deck");
                let mut icon = context.icon;
                icon.szTip.fill(0);
                let tooltip = wide(status);
                let count = tooltip.len().min(icon.szTip.len() - 1);
                icon.szTip[..count].copy_from_slice(&tooltip[..count]);
                Shell_NotifyIconW(NIM_MODIFY, &icon);
                return 0;
            }
            TRAY_MESSAGE => {
                match lparam as u32 {
                    WM_LBUTTONDBLCLK => {
                        PostMessageW(window, WM_COMMAND, OPEN, 0);
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        CheckMenuItem(
                            context.menu,
                            STARTUP as u32,
                            MF_BYCOMMAND
                                | if crate::startup::enabled() {
                                    MF_CHECKED
                                } else {
                                    MF_UNCHECKED
                                },
                        );
                        EnableMenuItem(
                            context.menu,
                            CONNECT as u32,
                            MF_BYCOMMAND
                                | if context.phone.busy.load(Ordering::Relaxed) {
                                    MF_GRAYED
                                } else {
                                    MF_ENABLED
                                },
                        );
                        let mut point = POINT::default();
                        GetCursorPos(&mut point);
                        SetForegroundWindow(window);
                        TrackPopupMenu(
                            context.menu,
                            TPM_RIGHTBUTTON,
                            point.x,
                            point.y,
                            0,
                            window,
                            ptr::null(),
                        );
                        PostMessageW(window, WM_NULL, 0, 0);
                    }
                    _ => {}
                }
                return 0;
            }
            WM_COMMAND => {
                match wparam & 0xffff {
                    OPEN => {
                        let result = ShellExecuteW(
                            window,
                            wide("open").as_ptr(),
                            wide(&context.url).as_ptr(),
                            ptr::null(),
                            ptr::null(),
                            SW_SHOWNORMAL,
                        );
                        if result as isize <= 32 {
                            show_error("Panel tarayıcıda açılamadı.");
                        }
                    }
                    CONNECT => {
                        context.phone.connect(&context.runtime, false);
                    }
                    STARTUP => {
                        if let Err(error) = crate::startup::set_enabled(!crate::startup::enabled())
                        {
                            show_error(&error);
                        }
                    }
                    EXIT => {
                        DestroyWindow(window);
                    }
                    _ => {}
                }
                return 0;
            }
            WM_CLOSE => {
                DestroyWindow(window);
                return 0;
            }
            WM_DESTROY => {
                Shell_NotifyIconW(NIM_DELETE, &context.icon);
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

async fn adb(executable: &std::ffi::OsStr, args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(args)
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| "ADB zaman aşımı: telefonun Wi-Fi bağlantısını kontrol edin.".to_string())?
        .map_err(|error| {
            format!("ADB çalıştırılamadı. Platform-Tools PATH üzerinde olmalı.\n{error}")
        })?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success()
        || text.contains("cannot connect")
        || text.contains("failed to connect")
        || text.contains("unable to connect")
        || text.contains("Error:")
        || text.contains("Error type")
    {
        return Err(format!("ADB: {}", text.trim()));
    }
    Ok(text)
}

async fn connect_phone(port: u16, url: &str) -> Result<(), String> {
    let executable = crate::startup::adb_path()?;
    let target =
        std::env::var("LIQUID_DECK_ADB_TARGET").unwrap_or_else(|_| "192.168.1.101:5555".into());
    adb(&executable, &["connect", &target]).await?;
    let state = adb(&executable, &["-s", &target, "get-state"]).await?;
    if state.trim() != "device" {
        return Err(format!("Note 4 hazır değil: {state}"));
    }
    let port = format!("tcp:{port}");
    adb(&executable, &["-s", &target, "reverse", &port, &port]).await?;
    adb(
        &executable,
        &[
            "-s",
            &target,
            "shell",
            "input",
            "keyevent",
            "KEYCODE_WAKEUP",
        ],
    )
    .await?;
    // adb shell joins arguments into a remote shell command: quote the URL,
    // including any percent-encoded token, rather than interpolating shell input.
    let url = format!("'{url}'");
    adb(
        &executable,
        &[
            "-s",
            &target,
            "shell",
            "am",
            "start",
            "-a",
            "android.intent.action.VIEW",
            "-n",
            "mark.via.gp/mark.via.Shell",
            "-d",
            &url,
        ],
    )
    .await?;
    Ok(())
}

pub fn run(config: Config) -> Result<(), String> {
    let Some(_instance) = crate::startup::Instance::acquire()? else {
        return Ok(());
    };
    if config.bind.port() == 0 {
        return Err("Tray mode requires a fixed port; use --console for port 0.".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        // Release short-lived ADB/file-operation helper threads promptly.
        .thread_keep_alive(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class_name = wide("LiquidDeckTray");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("Cannot register tray window".into());
        }
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return Err("Cannot create tray menu".into());
        }
        AppendMenuW(menu, MF_STRING, OPEN, wide("Paneli Aç").as_ptr());
        AppendMenuW(menu, MF_STRING, CONNECT, wide("Note 4’e Bağlan").as_ptr());
        AppendMenuW(
            menu,
            MF_STRING
                | if crate::startup::enabled() {
                    MF_CHECKED
                } else {
                    0
                },
            STARTUP,
            wide("Windows ile Başlat").as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        AppendMenuW(menu, MF_STRING, EXIT, wide("Çıkış").as_ptr());
        let mut context = Box::new(Context {
            menu,
            icon: NOTIFYICONDATAW::default(),
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            url: panel_url(&config, false),
            runtime: runtime.handle().clone(),
            phone: PhoneConnection {
                url: panel_url(&config, true),
                port: config.bind.port(),
                busy: Arc::new(AtomicBool::new(false)),
                status: Arc::new(Mutex::new("Liquid Deck")),
                window: 0,
            },
        });
        // An invisible top-level window receives Explorer restart notifications.
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("Liquid Deck").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            (&mut *context as *mut Context).cast(),
        );
        if window.is_null() {
            DestroyMenu(menu);
            return Err("Cannot create tray window".into());
        }
        context.phone.window = window as usize;
        context.icon.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        context.icon.hWnd = window;
        context.icon.uID = 1;
        context.icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        context.icon.uCallbackMessage = TRAY_MESSAGE;
        // A tiny deck glyph generated once; no image decoder or runtime assets.
        let mut pixels = [0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..32 {
                let offset = (y * 32 + x) * 4;
                let key = (5..14).contains(&x) || (18..27).contains(&x);
                let row = (5..14).contains(&y) || (18..27).contains(&y);
                let color = if key && row {
                    [230, 190, 40, 255]
                } else {
                    [24, 24, 24, 255]
                };
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
        let icon = CreateIcon(
            instance,
            32,
            32,
            1,
            32,
            [0u8; 128].as_ptr(),
            pixels.as_ptr(),
        );
        context.icon.hIcon = if icon.is_null() {
            LoadIconW(ptr::null_mut(), IDI_APPLICATION)
        } else {
            icon
        };
        let tooltip = wide("Liquid Deck — çift tık: paneli aç");
        context.icon.szTip[..tooltip.len()].copy_from_slice(&tooltip);
        if Shell_NotifyIconW(NIM_ADD, &context.icon) == 0 {
            DestroyWindow(window);
            DestroyMenu(menu);
            if !icon.is_null() {
                DestroyIcon(icon);
            }
            return Err("Cannot add tray icon".into());
        }
        let hwnd = window as usize;
        let worker = match std::thread::Builder::new().name("liquid-deck-server".into()).spawn(move || {
            let result = runtime.block_on(async move {
                let stop = Arc::new(Notify::new());
                let server = crate::run(config, stop.clone(), Some(ready_tx));
                tokio::pin!(server);
                tokio::select! {
                    result = &mut server => result,
                    _ = shutdown_rx => {
                        stop.notify_one();
                        // A stalled HTTP client must not keep Exit hanging forever.
                        tokio::time::timeout(Duration::from_secs(3), &mut server).await.unwrap_or(Ok(()))
                    }
                }
            });
            if let Err(error) = &result { show_error(error); }
            PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
            runtime.shutdown_timeout(Duration::from_secs(2));
            result
        }) {
            Ok(worker) => worker,
            Err(error) => { DestroyWindow(window); DestroyMenu(menu); if !icon.is_null() { DestroyIcon(icon); } return Err(error.to_string()); }
        };
        let phone = context.phone.clone();
        let handle = context.runtime.clone();
        context.runtime.spawn(async move {
            if ready_rx.await.is_ok() {
                if !std::env::var("LIQUID_DECK_AUTO_CONNECT")
                    .is_ok_and(|s| s.eq_ignore_ascii_case("off"))
                {
                    phone.connect(&handle, true);
                }
                crate::startup::start_sensor_task().await;
            }
        });
        let mut message = MSG::default();
        let failed = loop {
            let status = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
            if status <= 0 {
                break status == -1;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        };
        let _ = shutdown_tx.send(());
        if failed {
            DestroyWindow(window);
        }
        DestroyMenu(menu);
        if !icon.is_null() {
            DestroyIcon(icon);
        }
        // Keep callback context alive until the hidden window is destroyed.
        let _ = worker
            .join()
            .map_err(|_| "Server thread panicked".to_string())?;
        if failed {
            return Err("Tray message loop failed".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_are_local_and_token_is_encoded() {
        let mut config = Config {
            bind: "0.0.0.0:9876".parse().unwrap(),
            token: Some("a&b #é".into()),
            scripts_dir: None,
            sounds_dir: None,
            seed_sounds: false,
            deck_path: "boards.json".into(),
            state_path: "state.json".into(),
            ping_target: "1.1.1.1:443".into(),
            sample_ms: 2000,
            idle_ms: 5000,
        };
        assert_eq!(
            panel_url(&config, false),
            "http://127.0.0.1:9876/?token=a%26b%20%23%C3%A9"
        );
        config.bind = "[::1]:9876".parse().unwrap();
        assert!(panel_url(&config, false).starts_with("http://[::1]:9876/"));
        assert!(panel_url(&config, true).starts_with("http://127.0.0.1:9876/"));
    }
}
