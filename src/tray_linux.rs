//! Linux tray: a StatusNotifierItem served over D-Bus.
//!
//! GNOME needs the AppIndicator extension to show it, KDE and XFCE understand
//! the protocol natively. Nothing here links a GUI toolkit, so the binary keeps
//! its size and the server still runs headless with `--no-tray`.

use std::{io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf, process::Stdio, sync::Arc};

use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use ksni::{Icon, ToolTip, Tray, TrayMethods};
use tokio::sync::Notify;

use crate::Config;

// Generated from assets/tray.svg: ARGB32 in network byte order, no image decoder.
const PIXMAPS: [(i32, &[u8]); 3] = [
    (22, include_bytes!("../assets/tray-22.argb")),
    (32, include_bytes!("../assets/tray-32.argb")),
    (48, include_bytes!("../assets/tray-48.argb")),
];

struct DeckTray {
    url: String,
    shutdown: Arc<Notify>,
    autostart_file: Option<PathBuf>,
    autostart: bool,
}

impl DeckTray {
    fn open_panel(&self) {
        open_url(&self.url);
    }

    /// Creates or removes `~/.config/autostart/liquid-deck.desktop`.
    fn toggle_autostart(&mut self) {
        let Some(path) = self.autostart_file.clone() else {
            return;
        };
        if path.exists() {
            if let Err(error) = std::fs::remove_file(&path) {
                eprintln!("liquid-deck: cannot remove {}: {error}", path.display());
            }
        } else if let Some(parent) = path.parent() {
            let result = std::fs::create_dir_all(parent).and_then(|_| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)?
                    .write_all(autostart_entry().as_bytes())
            });
            if let Err(error) = result {
                eprintln!("liquid-deck: cannot write {}: {error}", path.display());
            }
        }
        self.autostart = path.exists();
    }

    fn quit(&self) {
        self.shutdown.notify_one();
    }
}

impl Tray for DeckTray {
    fn id(&self) -> String {
        "liquid-deck".into()
    }

    fn title(&self) -> String {
        "Liquid Deck".into()
    }

    fn icon_name(&self) -> String {
        if let Some(appdir) = std::env::var_os("APPDIR") {
            let path = PathBuf::from(appdir).join("liquid-deck.png");
            if path.is_file() {
                return path.to_string_lossy().into_owned();
            }
        }
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            });
        if let Some(path) = data.map(|base| base.join("icons/hicolor/512x512/apps/liquid-deck.png"))
        {
            if path.is_file() {
                return path.to_string_lossy().into_owned();
            }
        }
        if PathBuf::from("/usr/share/icons/hicolor/512x512/apps/liquid-deck.png").is_file() {
            return "liquid-deck".into();
        }
        // Standalone binaries retain the embedded pixmaps as a fallback.
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        PIXMAPS
            .iter()
            .map(|(size, data)| Icon {
                width: *size,
                height: *size,
                data: data.to_vec(),
            })
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Liquid Deck".into(),
            description: format!("Panel: {}\nSol tık: paneli aç", self.url),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.open_panel();
    }

    /// The host asks just before the menu opens, so the autostart tick is read
    /// from disk instead of being cached from startup.
    fn menu_about_to_show(&mut self) {
        self.autostart = self
            .autostart_file
            .as_ref()
            .map(|path| path.exists())
            .unwrap_or(false);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![
            StandardItem {
                label: "Paneli Aç".into(),
                icon_name: "web-browser".into(),
                activate: Box::new(|tray: &mut Self| tray.open_panel()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.url.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];

        if self.autostart_file.is_some() {
            items.push(
                CheckmarkItem {
                    label: "Oturum açılışında başlat".into(),
                    checked: self.autostart,
                    activate: Box::new(|tray: &mut Self| tray.toggle_autostart()),
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Çıkış".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.quit()),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Adds the icon when the session has a D-Bus and a tray host. A missing host
/// is not fatal: the server keeps serving and the panel stays reachable.
pub async fn start(config: &Config, shutdown: Arc<Notify>) {
    if config.bind.port() == 0 {
        return;
    }
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("liquid-deck: tray icon skipped (no session bus)");
        return;
    }

    let autostart_file = autostart_path();
    let tray = DeckTray {
        url: panel_url(config),
        shutdown,
        autostart: autostart_file
            .as_ref()
            .map(|path| path.exists())
            .unwrap_or(false),
        autostart_file,
    };

    match tray.spawn().await {
        Ok(handle) => {
            // Retain the service handle for the life of the runtime.
            tokio::spawn(async move {
                let _handle = handle;
                std::future::pending::<()>().await
            });
        }
        Err(error) => eprintln!("liquid-deck: tray icon unavailable: {error}"),
    }
}

fn panel_url(config: &Config) -> String {
    let host = if config.bind.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        match config.bind.ip() {
            std::net::IpAddr::V6(ip) => format!("[{ip}]"),
            ip => ip.to_string(),
        }
    };
    format!("http://{host}:{}/", config.bind.port())
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn autostart_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("autostart").join("liquid-deck.desktop"))
}

fn autostart_entry() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Liquid Deck\n\
         Icon=liquid-deck\n\
         Comment=Stream Deck paneli ve donanım panosu\n\
         Exec={}\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_line()
    )
}

/// The `.desktop` file has to survive the current working directory, so use the
/// absolute binary (the AppImage exposes itself through `$APPIMAGE`) and repeat
/// the arguments this process was started with.
fn exec_line() -> String {
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok());
    let mut parts = Vec::new();
    match exe {
        Some(exe) => parts.push(quote(&exe.to_string_lossy())),
        None => parts.push("liquid-deck".to_string()),
    }
    for arg in std::env::args().skip(1) {
        if arg == "--no-tray" || arg == "--console" {
            continue;
        }
        parts.push(quote(&arg));
    }
    parts.join(" ")
}

/// Desktop Entry `Exec` quoting: double quotes around anything that may contain
/// spaces, a literal `%` doubled so it is not read as a field code.
fn quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixmaps_have_expected_size_and_transparent_corners() {
        for (size, bytes) in PIXMAPS {
            assert_eq!(bytes.len(), (size * size * 4) as usize);
            assert_eq!(bytes[0], 0);
        }
    }

    #[test]
    fn desktop_exec_escapes_reserved_characters() {
        assert_eq!(quote("/home/me/Liquid Deck"), "\"/home/me/Liquid Deck\"");
        assert_eq!(quote("100%"), "\"100%%\"");
        assert_eq!(quote("$HOME"), "\"\\\\$HOME\"");
    }
}
