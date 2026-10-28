//! OS action executor.
//!
//! Every action is validated against a strict allow-list before a process is
//! spawned. The only way to run arbitrary code is through the scripts
//! directory (or an explicit `LIQUID_DECK_LAUNCH_*` / game-mode hook), which is
//! administrator-controlled configuration, never user input from the network.

use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::timeout};

use crate::{
    audio,
    deck::{self, ObsSettings},
    obs, sounds,
};

/// Actions advertised to the frontend (`GET /api/actions`, WS `hello`).
pub const ACTION_CATALOG: &[&str] = &[
    "key",
    "media",
    "volume",
    "mute",
    "launch",
    "script",
    "discord_mute",
    "discord_deafen",
    "game_mode",
    "terminal",
    "url",
    "sound",
    "obs",
    "audio",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActionRequest {
    /// Press a key combination, e.g. `{"action":"key","combo":"ctrl+shift+m"}`.
    Key { combo: String },
    /// `play_pause`, `next`, `previous`, `stop`.
    Media { command: String },
    /// `up` / `down` / `mute`, optionally with a step in percent.
    Volume {
        direction: String,
        #[serde(default)]
        step: Option<u8>,
    },
    /// Toggle the system mute state.
    Mute,
    /// `terminal`, `browser`, `files`, `calculator` or `editor`.
    Launch { target: String },
    /// Run an executable from the configured scripts directory.
    Script { name: String },
    /// Discord's default Ctrl+Shift+M shortcut.
    DiscordMute,
    /// Discord's default Ctrl+Shift+D shortcut.
    DiscordDeafen,
    /// Runs the configured game-mode hook and reports the new state.
    GameMode {
        #[serde(default)]
        enabled: bool,
    },
    /// Alias for `launch: terminal`.
    Terminal,
    /// Open an http(s) URL in the default browser.
    Url { url: String },
    /// Play a clip from the soundboard directory.
    Sound { name: String },
    /// Send one request to OBS over obs-websocket v5.
    Obs {
        #[serde(default)]
        op: String,
        #[serde(default)]
        target: String,
    },
    /// Switch output devices or change mixer levels.
    Audio {
        #[serde(default)]
        op: String,
        #[serde(default)]
        id: String,
        #[serde(default)]
        value: Option<u8>,
    },
}

impl ActionRequest {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Key { .. } => "key",
            Self::Media { .. } => "media",
            Self::Volume { .. } => "volume",
            Self::Mute => "mute",
            Self::Launch { .. } => "launch",
            Self::Script { .. } => "script",
            Self::DiscordMute => "discord_mute",
            Self::DiscordDeafen => "discord_deafen",
            Self::GameMode { .. } => "game_mode",
            Self::Terminal => "terminal",
            Self::Url { .. } => "url",
            Self::Sound { .. } => "sound",
            Self::Obs { .. } => "obs",
            Self::Audio { .. } => "audio",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ActionOutcome {
    pub ok: bool,
    pub action: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Executor {
    scripts_dir: Option<PathBuf>,
    sounds_dir: Option<PathBuf>,
    obs: Arc<RwLock<ObsSettings>>,
}

impl Executor {
    pub fn new(
        scripts_dir: Option<PathBuf>,
        sounds_dir: Option<PathBuf>,
        obs: ObsSettings,
    ) -> Self {
        Self {
            scripts_dir,
            sounds_dir,
            obs: Arc::new(RwLock::new(obs)),
        }
    }

    /// Keep the OBS credentials in sync when the editor saves a new config.
    pub fn set_obs(&self, settings: ObsSettings) {
        if let Ok(mut current) = self.obs.write() {
            *current = settings;
        }
    }

    pub async fn execute(&self, request: ActionRequest) -> ActionOutcome {
        let started = Instant::now();
        let name = request.name();

        let result = match request {
            ActionRequest::Key { combo } => key_combo(&combo).await,
            ActionRequest::Media { command } => media(&command).await,
            ActionRequest::Volume { direction, step } => {
                volume(&direction, step.unwrap_or(5)).await
            }
            ActionRequest::Mute => volume("mute", 0).await,
            ActionRequest::Launch { target } => launch(&target).await,
            ActionRequest::Terminal => launch("terminal").await,
            ActionRequest::Script { name } => self.run_script(&name).await,
            ActionRequest::DiscordMute => key_combo("ctrl+shift+m").await,
            ActionRequest::DiscordDeafen => key_combo("ctrl+shift+d").await,
            ActionRequest::GameMode { enabled } => game_mode(enabled).await,
            ActionRequest::Url { url } => open_url(&url).await,
            ActionRequest::Sound { name } => {
                sounds::play(self.sounds_dir.as_deref(), &name).await
            }
            ActionRequest::Obs { op, target } => {
                let settings = self
                    .obs
                    .read()
                    .map(|settings| settings.clone())
                    .unwrap_or_default();
                obs::execute(&settings, &op, &target).await
            }
            ActionRequest::Audio { op, id, value } => audio::apply(&op, &id, value).await,
        };

        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        match result {
            Ok(message) => ActionOutcome {
                ok: true,
                action: name,
                message,
                output: None,
                duration_ms,
            },
            Err(error) => ActionOutcome {
                ok: false,
                action: name,
                message: error,
                output: None,
                duration_ms,
            },
        }
    }

    async fn run_script(&self, name: &str) -> Result<String, String> {
        let root = self.scripts_dir.as_ref().ok_or_else(|| {
            "No scripts directory configured. Start liquid-deck with --scripts <dir>.".to_string()
        })?;

        if name.is_empty()
            || name.len() > 128
            || name.contains("..")
            || name.starts_with('/')
            || name.starts_with('\\')
            || name.contains('\0')
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
        {
            return Err("Invalid script name.".to_string());
        }

        let canonical_root = std::fs::canonicalize(root)
            .map_err(|_| format!("Scripts directory not found: {}", root.display()))?;
        let candidate = canonical_root.join(name);
        let canonical =
            std::fs::canonicalize(&candidate).map_err(|_| format!("Script not found: {name}"))?;

        if !canonical.starts_with(&canonical_root) {
            return Err("Script resolves outside the scripts directory.".to_string());
        }

        let metadata =
            std::fs::metadata(&canonical).map_err(|e| format!("Cannot inspect script: {e}"))?;
        if !metadata.is_file() {
            return Err("Script path is not a file.".to_string());
        }

        let extension = canonical
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();

        let command = script_command(&canonical, &extension)?;
        run_command(command, 10_000).await
    }
}

// ---------------------------------------------------------------------------
// Key combinations
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Super,
}

#[derive(Clone, PartialEq, Eq)]
enum Key {
    Char(char),
    Named(&'static str),
}

struct Combo {
    modifiers: Vec<Modifier>,
    key: Key,
}

fn parse_combo(input: &str) -> Result<Combo, String> {
    let raw = input.trim().to_ascii_lowercase();
    if raw.is_empty() || raw.len() > 48 {
        return Err("Key combination is empty or too long.".to_string());
    }

    let parts: Vec<&str> = raw.split('+').collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(format!("Invalid key combination: {input}"));
    }

    let (modifier_parts, key_parts) = parts.split_at(parts.len() - 1);
    let mut modifiers = Vec::with_capacity(modifier_parts.len());
    for part in modifier_parts {
        let modifier = match *part {
            "ctrl" | "control" => Modifier::Ctrl,
            "shift" => Modifier::Shift,
            "alt" | "option" | "opt" => Modifier::Alt,
            "super" | "meta" | "win" | "cmd" | "command" => Modifier::Super,
            other => return Err(format!("Unknown modifier: {other}")),
        };
        if !modifiers.contains(&modifier) {
            modifiers.push(modifier);
        }
    }
    if modifiers.len() > 4 {
        return Err("Too many modifiers.".to_string());
    }

    Ok(Combo {
        modifiers,
        key: parse_key(key_parts[0])?,
    })
}

fn parse_key(input: &str) -> Result<Key, String> {
    if input.chars().count() == 1 {
        let c = input.chars().next().unwrap();
        if c.is_ascii_alphanumeric() {
            return Ok(Key::Char(c.to_ascii_lowercase()));
        }
        return Err("Only letters and digits are supported as printable keys.".to_string());
    }

    let named = match input {
        "enter" | "return" => "return",
        "esc" | "escape" => "escape",
        "tab" => "tab",
        "space" | "spacebar" => "space",
        "backspace" | "back" => "backspace",
        "delete" | "del" => "delete",
        "insert" | "ins" => "insert",
        "home" => "home",
        "end" => "end",
        "pageup" | "pgup" => "pageup",
        "pagedown" | "pgdn" => "pagedown",
        "up" | "arrowup" => "up",
        "down" | "arrowdown" => "down",
        "left" | "arrowleft" => "left",
        "right" | "arrowright" => "right",
        "print" | "printscreen" | "prtsc" => "print",
        "f1" => "f1",
        "f2" => "f2",
        "f3" => "f3",
        "f4" => "f4",
        "f5" => "f5",
        "f6" => "f6",
        "f7" => "f7",
        "f8" => "f8",
        "f9" => "f9",
        "f10" => "f10",
        "f11" => "f11",
        "f12" => "f12",
        _ => return Err(format!("Unsupported key: {input}")),
    };

    Ok(Key::Named(named))
}

#[cfg(target_os = "linux")]
impl Combo {
    fn xdotool(&self) -> String {
        let mut parts: Vec<&str> = self
            .modifiers
            .iter()
            .map(|m| match m {
                Modifier::Ctrl => "ctrl",
                Modifier::Shift => "shift",
                Modifier::Alt => "alt",
                Modifier::Super => "super",
            })
            .collect();
        let key = match &self.key {
            Key::Char(c) => c.to_string(),
            Key::Named(name) => named_key(name, NamedLayout::Xdotool).to_string(),
        };
        parts.push(&key);
        parts.join("+")
    }

    fn wtype_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for modifier in &self.modifiers {
            args.push("-M".to_string());
            args.push(
                match modifier {
                    Modifier::Ctrl => "ctrl",
                    Modifier::Shift => "shift",
                    Modifier::Alt => "alt",
                    Modifier::Super => "logo",
                }
                .to_string(),
            );
        }
        args.push("-k".to_string());
        args.push(match &self.key {
            Key::Char(c) => c.to_string(),
            Key::Named(name) => named_key(name, NamedLayout::Xkb).to_string(),
        });
        for modifier in self.modifiers.iter().rev() {
            args.push("-m".to_string());
            args.push(
                match modifier {
                    Modifier::Ctrl => "ctrl",
                    Modifier::Shift => "shift",
                    Modifier::Alt => "alt",
                    Modifier::Super => "logo",
                }
                .to_string(),
            );
        }
        args
    }
}

#[cfg(target_os = "macos")]
impl Combo {
    fn applescript(&self) -> String {
        let using = if self.modifiers.is_empty() {
            String::new()
        } else {
            let mods: Vec<&str> = self
                .modifiers
                .iter()
                .map(|m| match m {
                    Modifier::Ctrl => "control down",
                    Modifier::Shift => "shift down",
                    Modifier::Alt => "option down",
                    Modifier::Super => "command down",
                })
                .collect();
            format!(" using {{{}}}", mods.join(", "))
        };

        match &self.key {
            Key::Char(c) => {
                format!("tell application \"System Events\" to keystroke \"{c}\"{using}")
            }
            Key::Named(name) => format!(
                "tell application \"System Events\" to key code {}{using}",
                mac_keycode(name)
            ),
        }
    }
}

#[cfg(target_os = "windows")]
impl Combo {
    fn send_keys(&self) -> Result<String, String> {
        if self.modifiers.contains(&Modifier::Super) {
            return Err("The Windows key is not supported by the SendKeys backend.".to_string());
        }
        let mut out = String::new();
        for modifier in &self.modifiers {
            out.push(match modifier {
                Modifier::Ctrl => '^',
                Modifier::Shift => '+',
                Modifier::Alt => '%',
                Modifier::Super => unreachable!(),
            });
        }
        match &self.key {
            Key::Char(c) => out.push(*c),
            Key::Named(name) => {
                out.push('{');
                out.push_str(named_key(name, NamedLayout::SendKeys));
                out.push('}');
            }
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum NamedLayout {
    Xdotool,
    Xkb,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    SendKeys,
}

fn named_key(name: &str, layout: NamedLayout) -> &'static str {
    match layout {
        NamedLayout::Xdotool => match name {
            "return" => "Return",
            "escape" => "Escape",
            "tab" => "Tab",
            "space" => "space",
            "backspace" => "BackSpace",
            "delete" => "Delete",
            "insert" => "Insert",
            "home" => "Home",
            "end" => "End",
            "pageup" => "Prior",
            "pagedown" => "Next",
            "up" => "Up",
            "down" => "Down",
            "left" => "Left",
            "right" => "Right",
            "print" => "Print",
            "f1" => "F1",
            "f2" => "F2",
            "f3" => "F3",
            "f4" => "F4",
            "f5" => "F5",
            "f6" => "F6",
            "f7" => "F7",
            "f8" => "F8",
            "f9" => "F9",
            "f10" => "F10",
            "f11" => "F11",
            "f12" => "F12",
            _ => "space",
        },
        NamedLayout::Xkb => match name {
            "return" => "Return",
            "escape" => "Escape",
            "tab" => "Tab",
            "space" => "space",
            "backspace" => "BackSpace",
            "delete" => "Delete",
            "insert" => "Insert",
            "home" => "Home",
            "end" => "End",
            "pageup" => "Prior",
            "pagedown" => "Next",
            "up" => "Up",
            "down" => "Down",
            "left" => "Left",
            "right" => "Right",
            "print" => "Print",
            "f1" => "F1",
            "f2" => "F2",
            "f3" => "F3",
            "f4" => "F4",
            "f5" => "F5",
            "f6" => "F6",
            "f7" => "F7",
            "f8" => "F8",
            "f9" => "F9",
            "f10" => "F10",
            "f11" => "F11",
            "f12" => "F12",
            _ => "space",
        },
        NamedLayout::SendKeys => match name {
            "return" => "ENTER",
            "escape" => "ESC",
            "tab" => "TAB",
            "space" => "SPACE",
            "backspace" => "BACKSPACE",
            "delete" => "DELETE",
            "insert" => "INSERT",
            "home" => "HOME",
            "end" => "END",
            "pageup" => "PGUP",
            "pagedown" => "PGDN",
            "up" => "UP",
            "down" => "DOWN",
            "left" => "LEFT",
            "right" => "RIGHT",
            "print" => "PRTSC",
            "f1" => "F1",
            "f2" => "F2",
            "f3" => "F3",
            "f4" => "F4",
            "f5" => "F5",
            "f6" => "F6",
            "f7" => "F7",
            "f8" => "F8",
            "f9" => "F9",
            "f10" => "F10",
            "f11" => "F11",
            "f12" => "F12",
            _ => "SPACE",
        },
    }
}

#[cfg(target_os = "macos")]
fn mac_keycode(name: &str) -> u16 {
    match name {
        "return" => 36,
        "escape" => 53,
        "tab" => 48,
        "space" => 49,
        "backspace" => 51,
        "delete" => 117,
        "insert" => 114,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        "up" => 126,
        "down" => 125,
        "left" => 123,
        "right" => 124,
        "print" => 105,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        _ => 49,
    }
}

// ---------------------------------------------------------------------------
// Per-platform actions
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
async fn key_combo(combo: &str) -> Result<String, String> {
    let parsed = parse_combo(combo)?;
    let keys = parsed.xdotool();

    let xdotool = run("xdotool", &["key", "--clearmodifiers", &keys], 2_500).await;
    if xdotool.is_ok() {
        return Ok(format!("Sent {combo}"));
    }

    let args = parsed.wtype_args();
    let wtype = run("wtype", &args, 2_500).await;
    if wtype.is_ok() {
        return Ok(format!("Sent {combo}"));
    }

    Err(format!(
        "Could not send keys. Install xdotool (X11) or wtype (Wayland). xdotool: {}; wtype: {}",
        xdotool.err().unwrap_or_default(),
        wtype.err().unwrap_or_default()
    ))
}

#[cfg(target_os = "macos")]
async fn key_combo(combo: &str) -> Result<String, String> {
    let parsed = parse_combo(combo)?;
    let script = parsed.applescript();
    run("osascript", &["-e", &script], 3_000)
        .await
        .map(|_| format!("Sent {combo}"))
        .map_err(|e| format!("Could not send keys (grant Accessibility permission): {e}"))
}

#[cfg(target_os = "windows")]
async fn key_combo(combo: &str) -> Result<String, String> {
    let parsed = parse_combo(combo)?;
    let keys = parsed.send_keys()?;
    let script =
        format!("$wshell = New-Object -ComObject WScript.Shell; $wshell.SendKeys('{keys}')");
    run_powershell(&script, 4_000)
        .await
        .map(|_| format!("Sent {combo}"))
}

#[cfg(target_os = "linux")]
async fn media(command: &str) -> Result<String, String> {
    let command = media_command(command)?;
    let playerctl = run("playerctl", &[command], 1_500).await;
    if playerctl.is_ok() {
        return Ok(format!("Media: {command}"));
    }

    let key = match command {
        "play-pause" => "XF86AudioPlay",
        "next" => "XF86AudioNext",
        "previous" => "XF86AudioPrev",
        _ => "XF86AudioStop",
    };
    let xdotool = run("xdotool", &["key", "--clearmodifiers", key], 1_500).await;
    if xdotool.is_ok() {
        return Ok(format!("Media: {command}"));
    }

    Err(format!(
        "Media control failed. Install playerctl or xdotool. playerctl: {}; xdotool: {}",
        playerctl.err().unwrap_or_default(),
        xdotool.err().unwrap_or_default()
    ))
}

#[cfg(target_os = "macos")]
async fn media(command: &str) -> Result<String, String> {
    let command = media_command(command)?;
    let verb = match command {
        "play-pause" => "playpause",
        "next" => "next track",
        "previous" => "previous track",
        _ => "pause",
    };

    let mut last_error = String::new();
    for app in ["Music", "Spotify"] {
        let script = format!("tell application \"{app}\" to {verb}");
        match run("osascript", &["-e", &script], 2_500).await {
            Ok(_) => return Ok(format!("Media: {command}")),
            Err(error) => last_error = error,
        }
    }

    Err(format!(
        "No controllable player found (tried Music and Spotify): {last_error}"
    ))
}

#[cfg(target_os = "windows")]
async fn media(command: &str) -> Result<String, String> {
    let command = media_command(command)?;
    let vk = match command {
        "play-pause" => 0xB3,
        "next" => 0xB0,
        "previous" => 0xB1,
        _ => 0xB2,
    };
    let script = format!(
        r#"Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);' -Name Native -Namespace LiquidDeck; [LiquidDeck.Native]::keybd_event({vk},0,0,[UIntPtr]::Zero); [LiquidDeck.Native]::keybd_event({vk},0,2,[UIntPtr]::Zero)"#
    );
    run_powershell(&script, 4_000)
        .await
        .map(|_| format!("Media: {command}"))
}

fn media_command(input: &str) -> Result<&'static str, String> {
    match input.to_ascii_lowercase().replace(' ', "_").as_str() {
        "play_pause" | "playpause" | "toggle" | "toggle_play" => Ok("play-pause"),
        "next" | "next_track" | "skip" => Ok("next"),
        "previous" | "prev" | "previous_track" => Ok("previous"),
        "stop" => Ok("stop"),
        other => Err(format!("Unknown media command: {other}")),
    }
}

async fn volume(direction: &str, step: u8) -> Result<String, String> {
    let direction = direction.to_ascii_lowercase();
    if !matches!(direction.as_str(), "up" | "down" | "mute" | "toggle") {
        return Err(format!("Unknown volume direction: {direction}"));
    }
    let step = step.clamp(1, 50);
    platform_volume(&direction, step).await
}

#[cfg(target_os = "linux")]
async fn platform_volume(direction: &str, step: u8) -> Result<String, String> {
    if direction == "mute" || direction == "toggle" {
        if run(
            "pactl",
            &["set-sink-mute", "@DEFAULT_SINK@", "toggle"],
            1_500,
        )
        .await
        .is_ok()
        {
            return Ok("Mute toggled".to_string());
        }
        if run(
            "wpctl",
            &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"],
            1_500,
        )
        .await
        .is_ok()
        {
            return Ok("Mute toggled".to_string());
        }
        if run(
            "amixer",
            &["-D", "pulse", "sset", "Master", "toggle"],
            1_500,
        )
        .await
        .is_ok()
        {
            return Ok("Mute toggled".to_string());
        }
        return Err("Could not toggle mute. Install pactl (PipeWire/PulseAudio) or amixer.".into());
    }

    let sign = if direction == "up" { "+" } else { "-" };
    let pactl_arg = format!("{sign}{step}%");
    if run(
        "pactl",
        &["set-sink-volume", "@DEFAULT_SINK@", &pactl_arg],
        1_500,
    )
    .await
    .is_ok()
    {
        return Ok(format!("Volume {direction} {step}%"));
    }

    let wpctl_arg = format!("{step}%{sign}");
    if run(
        "wpctl",
        &["set-volume", "@DEFAULT_AUDIO_SINK@", &wpctl_arg],
        1_500,
    )
    .await
    .is_ok()
    {
        return Ok(format!("Volume {direction} {step}%"));
    }

    let amixer_arg = format!("{step}%{sign}");
    if run(
        "amixer",
        &["-D", "pulse", "sset", "Master", &amixer_arg],
        1_500,
    )
    .await
    .is_ok()
    {
        return Ok(format!("Volume {direction} {step}%"));
    }

    Err("Could not change volume. Install pactl (PipeWire/PulseAudio) or amixer.".into())
}

#[cfg(target_os = "macos")]
async fn platform_volume(direction: &str, step: u8) -> Result<String, String> {
    let script = if direction == "mute" || direction == "toggle" {
        "set volume output muted (not (output muted of (get volume settings)))".to_string()
    } else {
        let operator = if direction == "up" { "+" } else { "-" };
        format!(
            "set v to output volume of (get volume settings)\n\
             set n to v {operator} {step}\n\
             if n > 100 then set n to 100\n\
             if n < 0 then set n to 0\n\
             set volume output volume n"
        )
    };

    run("osascript", &["-e", &script], 2_500).await?;
    if direction == "mute" || direction == "toggle" {
        Ok("Mute toggled".to_string())
    } else {
        Ok(format!("Volume {direction} {step}%"))
    }
}

#[cfg(target_os = "windows")]
async fn platform_volume(direction: &str, step: u8) -> Result<String, String> {
    let (vk, presses) = match direction {
        "mute" | "toggle" => (0xADu32, 1u32),
        "up" => (0xAF, (u32::from(step) / 2).max(1)),
        _ => (0xAE, (u32::from(step) / 2).max(1)),
    };
    let script = format!(
        r#"Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);' -Name Native -Namespace LiquidDeck; for ($i = 0; $i -lt {presses}; $i++) {{ [LiquidDeck.Native]::keybd_event({vk},0,0,[UIntPtr]::Zero); [LiquidDeck.Native]::keybd_event({vk},0,2,[UIntPtr]::Zero) }}"#
    );
    run_powershell(&script, 4_000).await?;
    if direction == "mute" || direction == "toggle" {
        Ok("Mute toggled".to_string())
    } else {
        Ok(format!("Volume {direction} {step}%"))
    }
}

#[cfg(target_os = "linux")]
fn platform_open_url(url: &str) -> Result<(), String> {
    spawn_detached("xdg-open", &[url])
}

#[cfg(target_os = "macos")]
fn platform_open_url(url: &str) -> Result<(), String> {
    spawn_detached("open", &[url])
}

#[cfg(target_os = "windows")]
fn platform_open_url(url: &str) -> Result<(), String> {
    // rundll32 takes the URL as a single argv element; no shell metacharacter
    // interpretation, unlike `cmd /C start`.
    spawn_detached("rundll32.exe", &["url.dll,FileProtocolHandler", url])
}

async fn open_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    deck::validate_url(url)?;
    platform_open_url(url)?;
    Ok("URL opened".to_string())
}

async fn launch(target: &str) -> Result<String, String> {
    let target = target.trim().to_ascii_lowercase();
    if !matches!(
        target.as_str(),
        "terminal" | "browser" | "files" | "calculator" | "editor"
    ) {
        return Err(format!("Unknown launch target: {target}"));
    }

    if let Some(custom) = env_launch(&target) {
        spawn_shell_detached(&custom)?;
        return Ok(format!("Launched {target}"));
    }

    platform_launch(&target)
}

fn env_launch(target: &str) -> Option<String> {
    env::var(format!(
        "LIQUID_DECK_LAUNCH_{}",
        target.to_ascii_uppercase()
    ))
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn platform_launch(target: &str) -> Result<String, String> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    match target {
        "terminal" => try_detached(&[
            ("x-terminal-emulator", &[][..]),
            ("gnome-terminal", &[][..]),
            ("konsole", &[][..]),
            ("xfce4-terminal", &[][..]),
            ("kitty", &[][..]),
            ("alacritty", &[][..]),
            ("foot", &[][..]),
            ("xterm", &[][..]),
        ]),
        "browser" => try_detached(&[("xdg-open", &["https://start.duckduckgo.com"][..])]),
        "files" => try_detached(&[("xdg-open", &[home.as_str()][..])]),
        "calculator" => try_detached(&[
            ("gnome-calculator", &[][..]),
            ("kcalc", &[][..]),
            ("xcalc", &[][..]),
        ]),
        "editor" => try_detached(&[
            ("code", &[][..]),
            ("gedit", &[][..]),
            ("kate", &[][..]),
            ("mousepad", &[][..]),
            ("xed", &[][..]),
        ]),
        _ => Err(format!("Unknown launch target: {target}")),
    }
}

#[cfg(target_os = "macos")]
fn platform_launch(target: &str) -> Result<String, String> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    match target {
        "terminal" => {
            spawn_detached("open", &["-a", "Terminal"]).map(|_| "Launched Terminal".into())
        }
        "browser" => spawn_detached("open", &["https://start.duckduckgo.com"])
            .map(|_| "Launched browser".into()),
        "files" => spawn_detached("open", &[home.as_str()]).map(|_| "Launched Finder".into()),
        "calculator" => {
            spawn_detached("open", &["-a", "Calculator"]).map(|_| "Launched Calculator".into())
        }
        "editor" => spawn_detached("open", &["-a", "TextEdit"]).map(|_| "Launched TextEdit".into()),
        _ => Err(format!("Unknown launch target: {target}")),
    }
}

#[cfg(target_os = "windows")]
fn platform_launch(target: &str) -> Result<String, String> {
    let home = env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    match target {
        "terminal" => try_detached(&[
            ("wt.exe", &[][..]),
            ("powershell.exe", &["-NoExit"][..]),
            ("cmd.exe", &[][..]),
        ]),
        "browser" => spawn_detached(
            "cmd.exe",
            &["/C", "start", "", "https://start.duckduckgo.com"],
        )
        .map(|_| "Launched browser".into()),
        "files" => {
            spawn_detached("explorer.exe", &[home.as_str()]).map(|_| "Launched Explorer".into())
        }
        "calculator" => spawn_detached("calc.exe", &[]).map(|_| "Launched Calculator".into()),
        "editor" => spawn_detached("notepad.exe", &[]).map(|_| "Launched Notepad".into()),
        _ => Err(format!("Unknown launch target: {target}")),
    }
}

async fn game_mode(enabled: bool) -> Result<String, String> {
    let key = if enabled {
        "LIQUID_DECK_GAME_MODE_ON"
    } else {
        "LIQUID_DECK_GAME_MODE_OFF"
    };

    match env::var(key) {
        Ok(command) if !command.trim().is_empty() => {
            run_shell(&command, 8_000).await?;
            Ok(format!("Game mode {}", if enabled { "on" } else { "off" }))
        }
        _ => Ok(format!(
            "Game mode {} (no hook configured)",
            if enabled { "on" } else { "off" }
        )),
    }
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

async fn run<S: AsRef<OsStr>>(
    program: &str,
    args: &[S],
    timeout_ms: u64,
) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(args);
    run_command(command, timeout_ms).await
}

async fn run_command(mut command: Command, timeout_ms: u64) -> Result<String, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start process: {error}"))?;

    match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                if stdout.is_empty() {
                    Ok("ok".to_string())
                } else {
                    Ok(truncate(&stdout, 1_200))
                }
            } else if stderr.is_empty() {
                Err(truncate(
                    if stdout.is_empty() {
                        "Command failed."
                    } else {
                        &stdout
                    },
                    1_200,
                ))
            } else {
                Err(truncate(&stderr, 1_200))
            }
        }
        Ok(Err(error)) => Err(format!("Process error: {error}")),
        Err(_) => Err(format!("Command timed out after {timeout_ms} ms.")),
    }
}

fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hidden(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{program}: {error}"))
}

fn try_detached(candidates: &[(&str, &[&str])]) -> Result<String, String> {
    let mut errors = Vec::new();
    for (program, args) in candidates {
        match spawn_detached(program, args) {
            Ok(()) => return Ok(format!("Launched {program}")),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "No launcher available ({}).",
        truncate(&errors.join("; "), 400)
    ))
}

#[cfg(target_os = "windows")]
fn spawn_shell_detached(script: &str) -> Result<(), String> {
    spawn_detached("cmd.exe", &["/C", script])
}

#[cfg(not(target_os = "windows"))]
fn spawn_shell_detached(script: &str) -> Result<(), String> {
    spawn_detached("sh", &["-c", script])
}

#[cfg(target_os = "windows")]
async fn run_shell(script: &str, timeout_ms: u64) -> Result<String, String> {
    run("cmd.exe", &["/C", script], timeout_ms).await
}

#[cfg(not(target_os = "windows"))]
async fn run_shell(script: &str, timeout_ms: u64) -> Result<String, String> {
    run("sh", &["-c", script], timeout_ms).await
}

#[cfg(target_os = "windows")]
async fn run_powershell(script: &str, timeout_ms: u64) -> Result<String, String> {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    hidden(&mut command);
    run_command(command, timeout_ms).await
}

#[cfg(target_os = "windows")]
fn script_command(path: &Path, extension: &str) -> Result<Command, String> {
    let mut command = match extension {
        "ps1" => {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
            command.arg(path);
            command
        }
        "bat" | "cmd" => {
            let mut command = Command::new("cmd.exe");
            command.arg("/C").arg(path);
            command
        }
        "py" => {
            let mut command = Command::new("python.exe");
            command.arg(path);
            command
        }
        _ => Command::new(path),
    };
    hidden(&mut command);
    Ok(command)
}

#[cfg(not(target_os = "windows"))]
fn script_command(path: &Path, extension: &str) -> Result<Command, String> {
    use std::os::unix::fs::PermissionsExt;

    let executable = std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false);

    let mut command = match extension {
        "sh" if !executable => {
            let mut command = Command::new("sh");
            command.arg(path);
            command
        }
        "py" if !executable => {
            let mut command = Command::new("python3");
            command.arg(path);
            command
        }
        _ => Command::new(path),
    };
    hidden(&mut command);
    Ok(command)
}

fn hidden(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

fn truncate(input: &str, max_chars: usize) -> String {
    let mut out: String = input.chars().take(max_chars).collect();
    if input.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("Liquid Deck supports Linux, macOS and Windows.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifier_combo() {
        let combo = parse_combo("Ctrl+Shift+M").unwrap();
        assert_eq!(combo.modifiers, vec![Modifier::Ctrl, Modifier::Shift]);
        assert!(matches!(combo.key, Key::Char('m')));
    }

    #[test]
    fn rejects_unsafe_combos() {
        assert!(parse_combo("ctrl+; rm -rf /").is_err());
        assert!(parse_combo("ctrl++m").is_err());
        assert!(parse_combo("hyper+m").is_err());
        assert!(parse_combo("").is_err());
    }

    #[test]
    fn parses_named_keys() {
        let combo = parse_combo("alt+f4").unwrap();
        assert!(matches!(combo.key, Key::Named("f4")));
    }

    #[test]
    fn normalizes_media_commands() {
        assert_eq!(media_command("play_pause").unwrap(), "play-pause");
        assert_eq!(media_command("Next Track").unwrap(), "next");
        assert_eq!(media_command("prev").unwrap(), "previous");
        assert!(media_command("format-disk").is_err());
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("abcdef", 3), "abc…");
        assert_eq!(truncate("abc", 3), "abc");
    }
}
