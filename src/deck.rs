//! Deck layout configuration and persistent toggle state.
//!
//! The whole deck (boards + buttons + integrations) lives in a single
//! `boards.json` file. The web editor writes it through `PUT /api/config`, the
//! server validates and normalises every field before it is persisted, and all
//! connected clients receive the new layout over the WebSocket.
//!
//! Toggle state (mic muted, recording, play/pause…) is stored separately in
//! `state.json` so re-saving the layout never resets the deck's live state.

use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const MAX_BOARDS: usize = 16;
pub const MAX_BUTTONS_PER_BOARD: usize = 32;
pub const MAX_COLUMNS: u8 = 8;
pub const MIN_COLUMNS: u8 = 2;

/// Actions the frontend is allowed to store in a button.
pub const ACTIONS: &[&str] = &[
    "discord_mute",
    "discord_deafen",
    "media",
    "volume",
    "mute",
    "key",
    "script",
    "launch",
    "terminal",
    "game_mode",
    "url",
    "sound",
    "obs",
    "audio",
    "board",
    "panel",
    "none",
];

pub const PANELS: &[&str] = &["editor", "settings", "audio"];

pub const OBS_OPS: &[&str] = &[
    "scene",
    "record_start",
    "record_stop",
    "record_toggle",
    "stream_start",
    "stream_stop",
    "stream_toggle",
    "mute_toggle",
    "mute_on",
    "mute_off",
    "replay_start",
    "replay_stop",
    "replay_save",
    "scene_collection",
    "test",
];

pub const AUDIO_OPS: &[&str] = &["output", "volume", "mute", "app_volume", "app_mute"];

pub const COLOR_PRESETS: &[&str] = &[
    "blue", "cyan", "violet", "pink", "green", "orange", "yellow", "red", "slate",
];

const DEFAULT_OBS_URL: &str = "ws://127.0.0.1:4455";

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DeckConfig {
    pub version: u32,
    pub boards: Vec<Board>,
    pub settings: Settings,
}

impl Default for DeckConfig {
    fn default() -> Self {
        default_deck()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub default_board: String,
    pub haptics: bool,
    pub show_subtitles: bool,
    pub obs: ObsSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_board: "gaming".to_string(),
            haptics: true,
            show_subtitles: true,
            obs: ObsSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ObsSettings {
    pub url: String,
    /// OBS WebSocket server password. Never sent to OBS logs.
    pub password: String,
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self {
            url: DEFAULT_OBS_URL.to_string(),
            password: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Preset name (`cyan`) or `#rrggbb`.
    pub accent: String,
    pub columns: u8,
    pub buttons: Vec<Button>,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Board".to_string(),
            icon: "grid".to_string(),
            accent: "#64d2ff".to_string(),
            columns: 4,
            buttons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Button {
    pub id: String,
    pub label: String,
    pub subtitle: String,
    /// Sprite icon name (`mic`), emoji or short text glyph.
    pub icon: String,
    pub active_label: String,
    pub active_icon: String,
    /// Preset name or `#rrggbb`; drives the glass glow.
    pub color: String,
    pub action: String,
    /// Primary action argument (URL, script, combo, board id, OBS scene…).
    pub value: String,
    /// Secondary argument (OBS source / audio sink / audio app index).
    pub arg: String,
    /// Volume step / audio percentage.
    pub amount: u8,
    /// When true the button keeps an on/off state that survives reloads.
    pub toggle: bool,
    /// Grid columns covered, 1–4.
    pub span: u8,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: "Button".to_string(),
            subtitle: String::new(),
            icon: "bolt".to_string(),
            active_label: String::new(),
            active_icon: String::new(),
            color: "cyan".to_string(),
            action: "none".to_string(),
            value: String::new(),
            arg: String::new(),
            amount: 5,
            toggle: false,
            span: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DeckState {
    /// Keyed as `board_id/button_id`.
    pub toggles: BTreeMap<String, bool>,
    pub active_board: String,
    pub updated_at: u64,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            toggles: BTreeMap::new(),
            active_board: String::new(),
            updated_at: 0,
        }
    }
}

impl DeckState {
    pub fn toggle_key(board: &str, button: &str) -> String {
        format!("{board}/{button}")
    }

    #[cfg(test)]
    pub fn is_active(&self, board: &str, button: &str) -> bool {
        self.toggles
            .get(&Self::toggle_key(board, button))
            .copied()
            .unwrap_or(false)
    }

    pub fn set(&mut self, board: &str, button: &str, active: bool) {
        self.toggles
            .insert(Self::toggle_key(board, button), active);
        self.updated_at = now_secs();
    }

    /// Drop toggles that belong to boards or buttons that no longer exist.
    pub fn retain_for(&mut self, config: &DeckConfig) {
        let mut valid = HashSet::new();
        for board in &config.boards {
            for button in &board.buttons {
                valid.insert(Self::toggle_key(&board.id, &button.id));
            }
        }
        self.toggles.retain(|key, _| valid.contains(key));
        if !config.boards.iter().any(|board| board.id == self.active_board) {
            self.active_board = config.settings.default_board.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Default deck
// ---------------------------------------------------------------------------

fn button(
    id: &str,
    label: &str,
    subtitle: &str,
    icon: &str,
    color: &str,
    action: &str,
    value: &str,
    arg: &str,
    toggle: bool,
    span: u8,
) -> Button {
    Button {
        id: id.to_string(),
        label: label.to_string(),
        subtitle: subtitle.to_string(),
        icon: icon.to_string(),
        color: color.to_string(),
        action: action.to_string(),
        value: value.to_string(),
        arg: arg.to_string(),
        toggle,
        span,
        ..Button::default()
    }
}

fn with_active(mut button: Button, label: &str, icon: &str) -> Button {
    button.active_label = label.to_string();
    button.active_icon = icon.to_string();
    button
}

pub fn default_deck() -> DeckConfig {
    let gaming = Board {
        id: "gaming".to_string(),
        name: "Gaming".to_string(),
        icon: "gamepad".to_string(),
        accent: "#ff9f0a".to_string(),
        columns: 4,
        buttons: vec![
            with_active(
                button(
                    "mic",
                    "Mic Mute",
                    "Discord · Ctrl ⇧ M",
                    "mic",
                    "orange",
                    "discord_mute",
                    "",
                    "",
                    true,
                    1,
                ),
                "Muted",
                "mic-off",
            ),
            with_active(
                button(
                    "deafen",
                    "Deafen",
                    "Discord · Ctrl ⇧ D",
                    "headphones",
                    "violet",
                    "discord_deafen",
                    "",
                    "",
                    true,
                    1,
                ),
                "Deafened",
                "headphones-off",
            ),
            with_active(
                button(
                    "game-mode",
                    "Game Mode",
                    "Performance profile",
                    "gamepad",
                    "pink",
                    "game_mode",
                    "",
                    "",
                    true,
                    1,
                ),
                "Active",
                "gamepad",
            ),
            with_active(
                button(
                    "obs-record",
                    "Record",
                    "OBS · toggle",
                    "record",
                    "red",
                    "obs",
                    "record_toggle",
                    "",
                    true,
                    1,
                ),
                "Recording",
                "record",
            ),
            button(
                "vol-down",
                "Volume −",
                "−5%",
                "volume-down",
                "slate",
                "volume",
                "down",
                "",
                false,
                1,
            ),
            button(
                "vol-up",
                "Volume +",
                "+5%",
                "volume-up",
                "slate",
                "volume",
                "up",
                "",
                false,
                1,
            ),
            button(
                "screenshot",
                "Screenshot",
                "Print screen",
                "camera",
                "cyan",
                "key",
                "print",
                "",
                false,
                1,
            ),
            button(
                "discord",
                "Discord",
                "Open voice chat",
                "chat",
                "blue",
                "url",
                "https://discord.com/app",
                "",
                false,
                1,
            ),
        ],
    };

    let workstation = Board {
        id: "workstation".to_string(),
        name: "Workstation".to_string(),
        icon: "terminal".to_string(),
        accent: "#0a84ff".to_string(),
        columns: 4,
        buttons: vec![
            button(
                "terminal",
                "Terminal",
                "Launch shell",
                "terminal",
                "green",
                "terminal",
                "",
                "",
                false,
                1,
            ),
            button(
                "editor",
                "Editor",
                "VS Code",
                "code",
                "blue",
                "launch",
                "editor",
                "",
                false,
                1,
            ),
            button(
                "files",
                "Files",
                "Home folder",
                "folder",
                "cyan",
                "launch",
                "files",
                "",
                false,
                1,
            ),
            button(
                "github",
                "GitHub",
                "Open in browser",
                "globe",
                "violet",
                "url",
                "https://github.com",
                "",
                false,
                1,
            ),
            button(
                "obs-scene",
                "Scene: Main",
                "OBS · switch",
                "obs",
                "pink",
                "obs",
                "scene",
                "Main",
                false,
                1,
            ),
            with_active(
                button(
                    "obs-record",
                    "Record",
                    "OBS · toggle",
                    "record",
                    "red",
                    "obs",
                    "record_toggle",
                    "",
                    true,
                    1,
                ),
                "Recording",
                "record",
            ),
            with_active(
                button(
                    "obs-stream",
                    "Go Live",
                    "OBS · toggle",
                    "stream",
                    "pink",
                    "obs",
                    "stream_toggle",
                    "",
                    true,
                    1,
                ),
                "Live",
                "stream",
            ),
            with_active(
                button(
                    "mute",
                    "Mute",
                    "System audio",
                    "volume-x",
                    "slate",
                    "mute",
                    "",
                    "",
                    true,
                    1,
                ),
                "Muted",
                "volume-x",
            ),
        ],
    };

    let media = Board {
        id: "media".to_string(),
        name: "Media".to_string(),
        icon: "music".to_string(),
        accent: "#bf5af2".to_string(),
        columns: 4,
        buttons: vec![
            with_active(
                button(
                    "play",
                    "Play / Pause",
                    "Now playing",
                    "play",
                    "blue",
                    "media",
                    "play_pause",
                    "",
                    true,
                    1,
                ),
                "Playing",
                "pause",
            ),
            button(
                "previous",
                "Previous",
                "Skip back",
                "previous",
                "violet",
                "media",
                "previous",
                "",
                false,
                1,
            ),
            button(
                "next",
                "Next",
                "Skip forward",
                "next",
                "cyan",
                "media",
                "next",
                "",
                false,
                1,
            ),
            with_active(
                button(
                    "mute",
                    "Mute",
                    "System audio",
                    "volume-x",
                    "slate",
                    "mute",
                    "",
                    "",
                    true,
                    1,
                ),
                "Muted",
                "volume-x",
            ),
            button(
                "vol-down",
                "Volume −",
                "−5%",
                "volume-down",
                "slate",
                "volume",
                "down",
                "",
                false,
                1,
            ),
            button(
                "vol-up",
                "Volume +",
                "+5%",
                "volume-up",
                "slate",
                "volume",
                "up",
                "",
                false,
                1,
            ),
            button(
                "click",
                "Click",
                "Soundboard",
                "sound",
                "orange",
                "sound",
                "click.wav",
                "",
                false,
                1,
            ),
            button(
                "mixer",
                "Mixer",
                "Audio devices",
                "speaker",
                "green",
                "panel",
                "audio",
                "",
                false,
                1,
            ),
        ],
    };

    let dev = Board {
        id: "dev".to_string(),
        name: "Dev".to_string(),
        icon: "code".to_string(),
        accent: "#30d158".to_string(),
        columns: 4,
        buttons: vec![
            button(
                "repo",
                "GitHub",
                "serkanagdas/liquid-deck",
                "github",
                "slate",
                "url",
                "https://github.com/serkanagdas/liquid-deck",
                "",
                false,
                1,
            ),
            button(
                "docs",
                "Docs",
                "docs.rs",
                "book",
                "blue",
                "url",
                "https://docs.rs",
                "",
                false,
                1,
            ),
            button(
                "localhost",
                "Localhost",
                "This deck",
                "globe",
                "cyan",
                "url",
                "http://localhost:8787",
                "",
                false,
                1,
            ),
            button(
                "obs-coding",
                "Scene: Coding",
                "OBS · switch",
                "obs",
                "pink",
                "obs",
                "scene",
                "Coding",
                false,
                1,
            ),
            button(
                "terminal",
                "Terminal",
                "Launch shell",
                "terminal",
                "green",
                "terminal",
                "",
                "",
                false,
                1,
            ),
            button(
                "chime",
                "Chime",
                "Soundboard",
                "sound",
                "orange",
                "sound",
                "chime.wav",
                "",
                false,
                1,
            ),
            button(
                "customize",
                "Customize",
                "Edit this deck",
                "sliders",
                "yellow",
                "panel",
                "editor",
                "",
                false,
                1,
            ),
            button(
                "settings",
                "Settings",
                "OBS & audio",
                "settings",
                "slate",
                "panel",
                "settings",
                "",
                false,
                1,
            ),
        ],
    };

    DeckConfig {
        version: CONFIG_VERSION,
        boards: vec![gaming, workstation, media, dev],
        settings: Settings::default(),
    }
}

// ---------------------------------------------------------------------------
// Validation / normalisation
// ---------------------------------------------------------------------------

impl DeckConfig {
    /// Sanitise and validate a configuration received from the editor.
    ///
    /// Structural problems (unknown action, broken board link, invalid URL)
    /// are rejected so the user gets a useful error. Cosmetic problems (odd
    /// colours, oversized labels) are clamped instead of failing.
    pub fn normalize(&mut self) -> Result<(), String> {
        self.version = CONFIG_VERSION;
        self.boards.truncate(MAX_BOARDS);
        if self.boards.is_empty() {
            return Err("The deck needs at least one board.".to_string());
        }

        let mut board_ids: HashSet<String> = HashSet::new();
        let mut board_names: Vec<String> = Vec::with_capacity(self.boards.len());

        for (index, board) in self.boards.iter_mut().enumerate() {
            board.name = clean_text(&board.name, 40);
            if board.name.is_empty() {
                board.name = format!("Board {}", index + 1);
            }
            board.id = unique_id(&board.id, &board.name, index, &mut board_ids);
            board.icon = clean_icon(&board.icon, "grid");
            board.accent = normalize_color(&board.accent, accent_fallback(index));
            board.columns = if board.columns == 0 {
                4
            } else {
                board.columns.clamp(MIN_COLUMNS, MAX_COLUMNS)
            };
            board.buttons.truncate(MAX_BUTTONS_PER_BOARD);
            board_names.push(board.name.clone());

            let mut button_ids: HashSet<String> = HashSet::new();
            for (button_index, button) in board.buttons.iter_mut().enumerate() {
                button.label = clean_text(&button.label, 48);
                if button.label.is_empty() {
                    button.label = format!("Button {}", button_index + 1);
                }
                button.subtitle = clean_text(&button.subtitle, 64);
                button.active_label = clean_text(&button.active_label, 48);
                button.icon = clean_icon(&button.icon, "bolt");
                button.active_icon = clean_icon(&button.active_icon, &button.icon);
                button.color = normalize_color(&button.color, "cyan");
                button.action = button.action.trim().to_ascii_lowercase();
                if !ACTIONS.contains(&button.action.as_str()) {
                    return Err(format!(
                        "{} › {}: unknown action “{}”.",
                        board.name, button.label, button.action
                    ));
                }
                button.value = clean_text(&button.value, value_limit(&button.action));
                button.arg = clean_text(&button.arg, 160);
                button.amount = if button.amount == 0 {
                    5
                } else {
                    button.amount.clamp(1, 100)
                };
                button.span = button.span.clamp(1, 4).min(u32::from(board.columns) as u8);
                button.id =
                    unique_id(&button.id, &button.label, button_index, &mut button_ids);

                validate_button(board.name.as_str(), button)?;
            }
        }

        let known: HashSet<&str> = board_ids.iter().map(String::as_str).collect();
        for board in &self.boards {
            for button in &board.buttons {
                if button.action == "board" && !known.contains(button.value.as_str()) {
                    return Err(format!(
                        "{} › {}: target board “{}” does not exist.",
                        board.name, button.label, button.value
                    ));
                }
            }
        }

        if !known.contains(self.settings.default_board.as_str()) {
            self.settings.default_board = self.boards[0].id.clone();
        }

        self.settings.obs.url = self.settings.obs.url.trim().to_string();
        if self.settings.obs.url.is_empty() {
            self.settings.obs.url = DEFAULT_OBS_URL.to_string();
        }
        validate_ws_url(&self.settings.obs.url)?;
        self.settings.obs.password = self.settings.obs.password.trim().to_string();
        if self.settings.obs.password.len() > 256 {
            return Err("OBS password is too long.".to_string());
        }

        Ok(())
    }
}

fn validate_button(board: &str, button: &Button) -> Result<(), String> {
    let context = || format!("{} › {}", board, button.label);
    match button.action.as_str() {
        "url" => {
            if button.value.is_empty() {
                return Err(format!("{}: a URL is required.", context()));
            }
            validate_url(&button.value)?;
        }
        "obs" => {
            if !OBS_OPS.contains(&button.value.as_str()) {
                return Err(format!(
                    "{}: unknown OBS operation “{}”.",
                    context(),
                    button.value
                ));
            }
        }
        "audio" => {
            if !AUDIO_OPS.contains(&button.value.as_str()) {
                return Err(format!(
                    "{}: unknown audio operation “{}”.",
                    context(),
                    button.value
                ));
            }
        }
        "panel" => {
            if !PANELS.contains(&button.value.as_str()) {
                return Err(format!(
                    "{}: unknown panel “{}”.",
                    context(),
                    button.value
                ));
            }
        }
        "volume" => {
            if !matches!(
                button.value.as_str(),
                "up" | "down" | "mute" | "toggle" | ""
            ) {
                return Err(format!(
                    "{}: volume direction must be up or down.",
                    context()
                ));
            }
        }
        "launch" => {
            if !matches!(
                button.value.as_str(),
                "terminal" | "browser" | "files" | "calculator" | "editor"
            ) {
                return Err(format!("{}: unknown launch target.", context()));
            }
        }
        "script" => {
            if !button.value.is_empty() && !valid_script_name(&button.value) {
                return Err(format!("{}: invalid script name.", context()));
            }
        }
        "sound" => {
            if !button.value.is_empty() && !valid_sound_name(&button.value) {
                return Err(format!("{}: invalid sound file name.", context()));
            }
        }
        _ => {}
    }
    Ok(())
}

fn value_limit(action: &str) -> usize {
    match action {
        "url" => 2_048,
        "obs" | "audio" | "panel" | "volume" | "media" | "launch" => 64,
        "key" => 48,
        _ => 512,
    }
}

pub fn validate_url(input: &str) -> Result<(), String> {
    let url = input.trim();
    if url.is_empty() || url.len() > 2_048 {
        return Err("URL is empty or too long.".to_string());
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("URL contains whitespace or control characters.".to_string());
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err("URL must start with http:// or https://.".to_string());
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err("Only http:// and https:// URLs are supported.".to_string());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err("URL has no host.".to_string());
    }
    Ok(())
}

pub fn validate_ws_url(input: &str) -> Result<(), String> {
    let url = input.trim();
    if url.is_empty() || url.len() > 512 {
        return Err("OBS WebSocket URL is empty or too long.".to_string());
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("OBS WebSocket URL contains whitespace.".to_string());
    }
    if !url.to_ascii_lowercase().starts_with("ws://") {
        return Err(
            "OBS WebSocket URL must start with ws:// (wss:// needs a TLS proxy).".to_string(),
        );
    }
    let host = url[5..].split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err("OBS WebSocket URL has no host.".to_string());
    }
    Ok(())
}

pub fn valid_script_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains("..")
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains('\0')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

pub fn valid_sound_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '-' | '.' | ' ' | '(' | ')' | '\'' | '&')
                || !c.is_ascii()
        })
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

pub fn default_data_dir() -> PathBuf {
    if let Ok(value) = env::var("LIQUID_DECK_DATA") {
        let value = value.trim();
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata).join("liquid-deck");
            }
        }
        if let Ok(profile) = env::var("USERPROFILE") {
            if !profile.trim().is_empty() {
                return PathBuf::from(profile).join(".liquid-deck");
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("liquid-deck");
            }
        }
        if let Ok(home) = env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home).join(".config").join("liquid-deck");
            }
        }
    }

    PathBuf::from(".liquid-deck")
}

pub fn load_config(path: &Path) -> Result<DeckConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut config: DeckConfig = serde_json::from_str(&raw)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    config.normalize()?;
    Ok(config)
}

/// Load a config file, falling back to the built-in deck.
/// The second value is a human-readable warning when the file was unusable.
pub fn load_or_default(path: &Path) -> (DeckConfig, Option<String>) {
    if !path.exists() {
        return (DeckConfig::default(), None);
    }
    match load_config(path) {
        Ok(config) => (config, None),
        Err(error) => (DeckConfig::default(), Some(error)),
    }
}

pub fn save_config(path: &Path, config: &DeckConfig) -> Result<(), String> {
    write_json_atomic(path, config)
}

pub fn load_state(path: &Path) -> DeckState {
    let Ok(raw) = fs::read_to_string(path) else {
        return DeckState::default();
    };
    let Ok(mut state) = serde_json::from_str::<DeckState>(&raw) else {
        return DeckState::default();
    };
    if state.toggles.len() > MAX_BOARDS * MAX_BUTTONS_PER_BOARD {
        state.toggles.clear();
    }
    state.toggles.retain(|key, _| {
        key.len() <= 96
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
    });
    state
}

pub fn save_state(path: &Path, state: &DeckState) -> Result<(), String> {
    write_json_atomic(path, state)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let data = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize configuration: {error}"))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, data)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600));
    }

    if let Err(error) = fs::rename(&temporary, path) {
        // Windows cannot replace an existing destination in one step.
        let _ = fs::remove_file(path);
        fs::rename(&temporary, path).map_err(|retry| {
            let _ = fs::remove_file(&temporary);
            format!("cannot replace {}: {error} ({retry})", path.display())
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn clean_text(input: &str, max_chars: usize) -> String {
    let filtered: String = input
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(max_chars)
        .collect();
    filtered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn clean_icon(input: &str, fallback: &str) -> String {
    let icon: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | '&' | '"' | '\''))
        .take(16)
        .collect();
    let icon = icon.trim().to_string();
    if icon.is_empty() {
        fallback.to_string()
    } else {
        icon
    }
}

pub fn normalize_color(input: &str, fallback: &str) -> String {
    let value = input.trim().to_ascii_lowercase();
    if COLOR_PRESETS.contains(&value.as_str()) {
        return value;
    }
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let expanded: String = hex.chars().flat_map(|c| [c, c]).collect();
            return format!("#{expanded}");
        }
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#{hex}");
        }
    }
    fallback.to_string()
}

fn accent_fallback(index: usize) -> &'static str {
    const ACCENTS: &[&str] = &["#ff9f0a", "#0a84ff", "#bf5af2", "#30d158"];
    ACCENTS[index % ACCENTS.len()]
}

fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = true;
    for character in input.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            out.push(character);
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
        if out.len() >= 32 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn unique_id(raw: &str, name: &str, index: usize, used: &mut HashSet<String>) -> String {
    let mut base = slug(raw);
    if base.is_empty() {
        base = slug(name);
    }
    if base.is_empty() {
        base = format!("item-{}", index + 1);
    }
    let mut candidate = base.clone();
    let mut suffix = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    used.insert(candidate.clone());
    candidate
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_deck_is_valid() {
        let mut config = DeckConfig::default();
        config.normalize().expect("default deck must validate");
        assert_eq!(config.boards.len(), 4);
        assert!(config.boards.iter().any(|board| board.id == "gaming"));
        assert!(config.boards.iter().any(|board| board.id == "dev"));
        assert_eq!(config.settings.default_board, "gaming");
        for board in &config.boards {
            assert!(!board.buttons.is_empty());
            for button in &board.buttons {
                assert!(ACTIONS.contains(&button.action.as_str()));
                assert!(button.span <= board.columns);
            }
        }
    }

    #[test]
    fn rejects_unknown_action_and_bad_url() {
        let mut config = DeckConfig::default();
        config.boards[0].buttons[0].action = "format_disk".to_string();
        assert!(config.normalize().is_err());

        let mut config = DeckConfig::default();
        config.boards[0].buttons[0] = Button {
            action: "url".to_string(),
            value: "javascript:alert(1)".to_string(),
            ..Button::default()
        };
        assert!(config.normalize().is_err());

        let mut config = DeckConfig::default();
        config.boards[0].buttons[0] = Button {
            action: "url".to_string(),
            value: "https://example.com/path?q=1".to_string(),
            ..Button::default()
        };
        assert!(config.normalize().is_ok());
    }

    #[test]
    fn board_links_are_checked() {
        let mut config = DeckConfig::default();
        config.boards[0].buttons[0] = Button {
            action: "board".to_string(),
            value: "missing".to_string(),
            ..Button::default()
        };
        assert!(config.normalize().is_err());

        config.boards[0].buttons[0].value = "media".to_string();
        assert!(config.normalize().is_ok());
    }

    #[test]
    fn ids_are_slugged_and_unique() {
        let mut config = DeckConfig::default();
        config.boards[0].id = "  Gaming Board!  ".to_string();
        config.boards[0].buttons[0].id = "".to_string();
        config.boards[0].buttons[1].id = "".to_string();
        config.normalize().unwrap();
        assert_eq!(config.boards[0].id, "gaming-board");
        assert_eq!(config.boards[0].buttons[0].id, "mic-mute");
        assert_eq!(config.boards[0].buttons[1].id, "deafen");
    }

    #[test]
    fn clamps_layout_values() {
        let mut config = DeckConfig::default();
        config.boards[0].columns = 99;
        config.boards[0].buttons[0].span = 9;
        config.boards[0].buttons[1].amount = 0;
        config.normalize().unwrap();
        assert_eq!(config.boards[0].columns, MAX_COLUMNS);
        assert_eq!(config.boards[0].buttons[0].span, 4);
        assert_eq!(config.boards[0].buttons[1].amount, 5);
    }

    #[test]
    fn colors_are_normalised() {
        assert_eq!(normalize_color("CYAN", "blue"), "cyan");
        assert_eq!(normalize_color("#ABC", "blue"), "#aabbcc");
        assert_eq!(normalize_color("#12ab34", "blue"), "#12ab34");
        assert_eq!(normalize_color("chartreuse", "blue"), "blue");
        assert_eq!(normalize_color("#zzzzzz", "blue"), "blue");
    }

    #[test]
    fn urls_are_restricted() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://192.168.1.10:8787/path").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("https://exa mple.com").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_ws_url("ws://127.0.0.1:4455").is_ok());
        assert!(validate_ws_url("wss://example.com").is_err());
    }

    #[test]
    fn sound_names_stay_inside_the_library() {
        assert!(valid_sound_name("click.wav"));
        assert!(valid_sound_name("My Clip (1).mp3"));
        assert!(!valid_sound_name("../secret.wav"));
        assert!(!valid_sound_name("sub/dir.wav"));
        assert!(!valid_sound_name("clip\0.wav"));
    }

    #[test]
    fn toggle_state_is_scoped_per_board() {
        let mut state = DeckState::default();
        state.set("gaming", "mic", true);
        assert!(state.is_active("gaming", "mic"));
        assert!(!state.is_active("media", "mic"));
        assert_eq!(state.toggles.len(), 1);
    }

    #[test]
    fn state_prunes_deleted_buttons() {
        let config = DeckConfig::default();
        let mut state = DeckState::default();
        state.active_board = "gaming".to_string();
        state.set("gaming", "mic", true);
        state.set("gaming", "gone", true);
        state.set("deleted", "button", true);
        state.retain_for(&config);
        assert!(state.is_active("gaming", "mic"));
        assert!(!state.toggles.contains_key("gaming/gone"));
        assert!(!state.toggles.contains_key("deleted/button"));
    }

    #[test]
    fn config_round_trips_through_disk() {
        let dir = env::temp_dir().join(format!("liquid-deck-test-{}", std::process::id()));
        let path = dir.join("boards.json");
        let mut config = DeckConfig::default();
        config.normalize().unwrap();
        save_config(&path, &config).unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(config, loaded);

        let state_path = dir.join("state.json");
        let mut state = DeckState::default();
        state.active_board = "media".to_string();
        state.set("media", "play", true);
        save_state(&state_path, &state).unwrap();
        let loaded_state = load_state(&state_path);
        assert_eq!(loaded_state.toggles, state.toggles);
        assert_eq!(loaded_state.active_board, "media");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn defaults_fill_missing_json_fields() {
        let config: DeckConfig = serde_json::from_str(
            r#"{"boards":[{"name":"Only","buttons":[{"label":"Hi"}]}]}"#,
        )
        .unwrap();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.boards.len(), 1);
        assert_eq!(config.boards[0].columns, 4);
        assert_eq!(config.boards[0].buttons[0].action, "none");
        assert_eq!(config.settings.default_board, "gaming");
    }
}
