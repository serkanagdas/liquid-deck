//! Soundboard file library and host-side playback.
//!
//! Audio clips live in a directory (`--sounds`, or `<data>/sounds`). The
//! server only ever resolves names inside that directory — no paths from the
//! network are trusted — and hands playback to the first available native
//! audio player.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::Serialize;
use tokio::process::Command;

pub const EXTENSIONS: &[&str] = &[
    "wav", "mp3", "ogg", "oga", "flac", "m4a", "aac", "opus", "aiff", "aif",
];

/// Tiny built-in soundboard clips, installed on first run.
const BUILT_IN: &[(&str, &[u8])] = &[
    ("click.wav", include_bytes!("../assets/sounds/click.wav")),
    ("chime.wav", include_bytes!("../assets/sounds/chime.wav")),
    ("alert.wav", include_bytes!("../assets/sounds/alert.wav")),
];

pub const MAX_SOUND_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SoundEntry {
    pub name: String,
    pub size: u64,
}

/// Populate a fresh sounds directory with the built-in clips.
pub fn install_defaults(dir: &Path) -> Result<(), String> {
    let empty = if dir.exists() {
        fs::read_dir(dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    } else {
        true
    };
    if !empty {
        return Ok(());
    }

    fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    for (name, bytes) in BUILT_IN {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, bytes)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

pub fn list(dir: Option<&Path>) -> Vec<SoundEntry> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut sounds: Vec<SoundEntry> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() || !allowed_extension(&path) {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_string();
            if !crate::deck::valid_sound_name(&name) {
                return None;
            }
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            Some(SoundEntry { name, size })
        })
        .take(500)
        .collect();

    sounds.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    sounds
}

pub fn resolve(dir: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    let dir = dir.ok_or_else(|| {
        "Soundboard is disabled. Start liquid-deck with --sounds <directory>.".to_string()
    })?;
    if !crate::deck::valid_sound_name(name) {
        return Err("Invalid sound name.".to_string());
    }

    let root = fs::canonicalize(dir)
        .map_err(|_| format!("Sounds directory not found: {}", dir.display()))?;
    let candidate = root.join(name);
    let path = fs::canonicalize(&candidate)
        .map_err(|_| format!("Sound not found: {name}"))?;
    if !path.starts_with(&root) {
        return Err("Sound resolves outside the sounds directory.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| format!("Cannot inspect sound: {error}"))?;
    if !metadata.is_file() {
        return Err("Sound path is not a file.".to_string());
    }
    if metadata.len() > MAX_SOUND_BYTES {
        return Err("Sound file is too large (32 MB limit).".to_string());
    }
    if !allowed_extension(&path) {
        return Err("Unsupported audio format.".to_string());
    }
    Ok(path)
}

pub fn media_type(path: &Path) -> &'static str {
    match extension(path).as_str() {
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "aiff" | "aif" => "audio/aiff",
        _ => "audio/wav",
    }
}

pub async fn play(dir: Option<&Path>, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("No sound selected for this button.".to_string());
    }
    let path = resolve(dir, name)?;
    let display = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(name)
        .to_string();
    platform_play(&path)?;
    Ok(format!("Playing {display}"))
}

fn allowed_extension(path: &Path) -> bool {
    EXTENSIONS.contains(&extension(path).as_str())
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn detached(program: &str, args: &[&OsStr]) -> Result<(), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{program}: {error}"))
}

#[cfg(target_os = "linux")]
fn platform_play(path: &Path) -> Result<(), String> {
    let file = path.as_os_str();
    let attempts: &[(&str, &[&OsStr])] = &[
        ("paplay", &[file]),
        ("pw-play", &[file]),
        (
            "ffplay",
            &[
                OsStr::new("-nodisp"),
                OsStr::new("-autoexit"),
                OsStr::new("-loglevel"),
                OsStr::new("quiet"),
                file,
            ],
        ),
        (
            "mpv",
            &[
                OsStr::new("--no-video"),
                OsStr::new("--really-quiet"),
                file,
            ],
        ),
        (
            "aplay",
            &[OsStr::new("-q"), file],
        ),
    ];

    let mut errors = Vec::new();
    for (program, args) in attempts {
        match detached(program, args) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "No audio player available (tried paplay, pw-play, ffplay, mpv, aplay). {}",
        errors.join("; ")
    ))
}

#[cfg(target_os = "macos")]
fn platform_play(path: &Path) -> Result<(), String> {
    detached("afplay", &[path.as_os_str()])
}

#[cfg(target_os = "windows")]
fn platform_play(path: &Path) -> Result<(), String> {
    let script = "$p = New-Object System.Windows.Media.MediaPlayer; \
                  $p.Open([uri]$env:LIQUID_SOUND); $p.Play(); Start-Sleep -Seconds 10";
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .env("LIQUID_SOUND", path.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("powershell.exe: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_mapped() {
        assert_eq!(media_type(Path::new("a.mp3")), "audio/mpeg");
        assert_eq!(media_type(Path::new("a.ogg")), "audio/ogg");
        assert_eq!(media_type(Path::new("a.wav")), "audio/wav");
        assert!(allowed_extension(Path::new("a.flac")));
        assert!(!allowed_extension(Path::new("a.exe")));
    }

    #[test]
    fn traversal_is_rejected() {
        let dir = std::env::temp_dir().join(format!("liquid-deck-sounds-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ok.wav"), b"RIFF").unwrap();
        assert!(resolve(Some(&dir), "ok.wav").is_ok());
        assert!(resolve(Some(&dir), "../ok.wav").is_err());
        assert!(resolve(Some(&dir), "missing.wav").is_err());
        assert!(resolve(None, "ok.wav").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn built_in_clips_install_once() {
        let dir = std::env::temp_dir().join(format!("liquid-deck-builtin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        install_defaults(&dir).unwrap();
        let sounds = list(Some(&dir));
        assert!(sounds.iter().any(|sound| sound.name == "click.wav"));
        assert!(sounds.iter().any(|sound| sound.name == "chime.wav"));
        fs::write(dir.join("custom.wav"), b"RIFF").unwrap();
        install_defaults(&dir).unwrap();
        assert!(dir.join("custom.wav").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
