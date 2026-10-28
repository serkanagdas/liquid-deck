//! Audio output routing and per-application mixer helpers.
//!
//! On Linux everything goes through `pactl` (PipeWire's PulseAudio shim or
//! PulseAudio itself), which is present on almost every modern desktop. macOS
//! uses `SwitchAudioSource` when installed plus AppleScript for level control.
//! Windows has no dependency-free way to enumerate endpoints, so the mixer
//! reports that it is unavailable instead of pretending.

use std::time::Duration;

use serde::Serialize;
use tokio::{process::Command, time::timeout};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub description: String,
    pub volume: u8,
    pub muted: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioApp {
    /// sink-input index, used for per-app volume/mute.
    pub id: String,
    pub app: String,
    pub title: String,
    pub volume: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AudioSnapshot {
    pub supported: bool,
    pub backend: String,
    pub devices: Vec<AudioDevice>,
    pub apps: Vec<AudioApp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub async fn snapshot() -> AudioSnapshot {
    platform_snapshot().await
}

pub async fn apply(op: &str, id: &str, value: Option<u8>) -> Result<String, String> {
    let op = op.trim().to_ascii_lowercase();
    if !crate::deck::AUDIO_OPS.contains(&op.as_str()) {
        return Err(format!("Unknown audio operation: {op}"));
    }
    platform_apply(&op, id.trim(), value.unwrap_or(50).min(100)).await
}

// ---------------------------------------------------------------------------
// Linux (pactl)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
async fn platform_snapshot() -> AudioSnapshot {
    let default = run("pactl", &["get-default-sink"], 2_000)
        .await
        .unwrap_or_default();
    let default = default.trim().to_string();

    let sinks = match run("pactl", &["list", "sinks"], 3_000).await {
        Ok(text) => text,
        Err(error) => {
            return AudioSnapshot {
                supported: false,
                backend: "pactl".to_string(),
                message: Some(format!(
                    "pactl is unavailable ({error}). Install pipewire-pulse or pulseaudio-utils."
                )),
                ..AudioSnapshot::default()
            };
        }
    };

    let inputs = run("pactl", &["list", "sink-inputs"], 3_000)
        .await
        .unwrap_or_default();

    AudioSnapshot {
        supported: true,
        backend: "pactl".to_string(),
        devices: parse_sinks(&sinks, &default),
        apps: parse_sink_inputs(&inputs),
        message: None,
    }
}

#[cfg(target_os = "linux")]
async fn platform_apply(op: &str, id: &str, value: u8) -> Result<String, String> {
    match op {
        "output" => {
            if id.is_empty() {
                return Err("Select an output device first.".to_string());
            }
            let snapshot = platform_snapshot().await;
            if !snapshot.devices.iter().any(|device| device.id == id) {
                return Err("Unknown output device. Refresh the mixer.".to_string());
            }
            run("pactl", &["set-default-sink", id], 2_500).await?;
            Ok("Output switched".to_string())
        }
        "volume" => {
            let target = sink_or_default(id).await?;
            let level = format!("{value}%");
            run("pactl", &["set-sink-volume", &target, &level], 2_500).await?;
            Ok(format!("Volume {value}%"))
        }
        "mute" => {
            let target = sink_or_default(id).await?;
            run("pactl", &["set-sink-mute", &target, "toggle"], 2_500).await?;
            Ok("Output mute toggled".to_string())
        }
        "app_volume" => {
            let index = validate_index(id)?;
            let level = format!("{value}%");
            run(
                "pactl",
                &["set-sink-input-volume", &index, &level],
                2_500,
            )
            .await?;
            Ok(format!("App volume {value}%"))
        }
        "app_mute" => {
            let index = validate_index(id)?;
            run("pactl", &["set-sink-input-mute", &index, "toggle"], 2_500).await?;
            Ok("App mute toggled".to_string())
        }
        other => Err(format!("Unknown audio operation: {other}")),
    }
}

#[cfg(target_os = "linux")]
async fn sink_or_default(id: &str) -> Result<String, String> {
    if !id.is_empty() {
        let snapshot = platform_snapshot().await;
        if !snapshot.devices.iter().any(|device| device.id == id) {
            return Err("Unknown output device. Refresh the mixer.".to_string());
        }
        return Ok(id.to_string());
    }
    let default = run("pactl", &["get-default-sink"], 2_000)
        .await?
        .trim()
        .to_string();
    if default.is_empty() {
        return Err("No default output device.".to_string());
    }
    Ok(default)
}

#[cfg(target_os = "linux")]
fn validate_index(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 16 || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid application mixer id.".to_string());
    }
    Ok(id.to_string())
}

#[cfg(target_os = "linux")]
pub fn parse_sinks(text: &str, default: &str) -> Vec<AudioDevice> {
    #[derive(Default)]
    struct Block {
        name: String,
        description: String,
        volume: u8,
        muted: bool,
    }

    let mut devices = Vec::new();
    let mut current: Option<Block> = None;

    let flush = |current: Option<Block>, devices: &mut Vec<AudioDevice>| {
        if let Some(block) = current {
            if !block.name.is_empty() {
                devices.push(AudioDevice {
                    is_default: !default.is_empty() && block.name == default,
                    id: block.name,
                    description: if block.description.is_empty() {
                        "Audio output".to_string()
                    } else {
                        block.description
                    },
                    volume: block.volume,
                    muted: block.muted,
                });
            }
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink #") {
            flush(current.take(), &mut devices);
            current = Some(Block::default());
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Name:") {
            if block.name.is_empty() {
                block.name = value.trim().to_string();
            }
        } else if let Some(value) = trimmed.strip_prefix("Description:") {
            if block.description.is_empty() {
                block.description = value.trim().to_string();
            }
        } else if let Some(value) = trimmed.strip_prefix("Mute:") {
            block.muted = value.trim().eq_ignore_ascii_case("yes");
        } else if trimmed.starts_with("Volume:") {
            if let Some(percent) = first_percent(trimmed) {
                block.volume = percent;
            }
        }
    }
    flush(current.take(), &mut devices);
    devices
}

#[cfg(target_os = "linux")]
pub fn parse_sink_inputs(text: &str) -> Vec<AudioApp> {
    #[derive(Default)]
    struct Block {
        index: String,
        app: String,
        title: String,
        volume: u8,
        muted: bool,
    }

    let mut apps = Vec::new();
    let mut current: Option<Block> = None;

    let flush = |current: Option<Block>, apps: &mut Vec<AudioApp>| {
        if let Some(block) = current {
            if !block.index.is_empty() {
                let app = if block.app.is_empty() {
                    if block.title.is_empty() {
                        format!("Application #{}", block.index)
                    } else {
                        block.title.clone()
                    }
                } else {
                    block.app
                };
                apps.push(AudioApp {
                    id: block.index,
                    app,
                    title: block.title,
                    volume: block.volume,
                    muted: block.muted,
                });
            }
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink Input #") {
            flush(current.take(), &mut apps);
            current = Some(Block {
                index: trimmed
                    .trim_start_matches("Sink Input #")
                    .trim()
                    .to_string(),
                ..Block::default()
            });
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Mute:") {
            block.muted = value.trim().eq_ignore_ascii_case("yes");
        } else if trimmed.starts_with("Volume:") {
            if let Some(percent) = first_percent(trimmed) {
                block.volume = percent;
            }
        } else if let Some(value) = trimmed.strip_prefix("application.name =") {
            block.app = unquote(value);
        } else if let Some(value) = trimmed.strip_prefix("media.name =") {
            block.title = unquote(value);
        }
    }
    flush(current.take(), &mut apps);
    apps
}

fn first_percent(line: &str) -> Option<u8> {
    let marker = line.find('%')?;
    let prefix = &line[..marker];
    let digits: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse::<u8>().ok().map(|value| value.min(100))
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
async fn platform_snapshot() -> AudioSnapshot {
    let list = run("SwitchAudioSource", &["-a", "-t", "output"], 2_500).await;
    match list {
        Ok(output) => {
            let current = run("SwitchAudioSource", &["-c", "-t", "output"], 2_000)
                .await
                .unwrap_or_default();
            let current = current.trim().to_string();
            let devices = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| AudioDevice {
                    id: line.to_string(),
                    description: line.to_string(),
                    volume: 0,
                    muted: false,
                    is_default: !current.is_empty() && line == current,
                })
                .collect();
            AudioSnapshot {
                supported: true,
                backend: "SwitchAudioSource".to_string(),
                devices,
                apps: Vec::new(),
                message: None,
            }
        }
        Err(_) => AudioSnapshot {
            supported: false,
            backend: "SwitchAudioSource".to_string(),
            message: Some(
                "Install SwitchAudioSource (brew install switchaudio-osx) to switch outputs."
                    .to_string(),
            ),
            ..AudioSnapshot::default()
        },
    }
}

#[cfg(target_os = "macos")]
async fn platform_apply(op: &str, id: &str, value: u8) -> Result<String, String> {
    match op {
        "output" => {
            if id.is_empty() {
                return Err("Select an output device first.".to_string());
            }
            run("SwitchAudioSource", &["-s", id, "-t", "output"], 2_500).await?;
            Ok("Output switched".to_string())
        }
        "volume" => {
            let script = format!("set volume output volume {value}");
            run("osascript", &["-e", &script], 2_500).await?;
            Ok(format!("Volume {value}%"))
        }
        "mute" => {
            let script = "set volume output muted (not (output muted of (get volume settings)))";
            run("osascript", &["-e", script], 2_500).await?;
            Ok("Output mute toggled".to_string())
        }
        _ => Err("Per-application mixing is not available on macOS.".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
async fn platform_snapshot() -> AudioSnapshot {
    AudioSnapshot {
        supported: false,
        backend: "windows".to_string(),
        message: Some(
            "Endpoint switching needs the AudioDeviceCmdlets PowerShell module. \
             Use volume buttons or assign a script action instead."
                .to_string(),
        ),
        ..AudioSnapshot::default()
    }
}

#[cfg(target_os = "windows")]
async fn platform_apply(_op: &str, _id: &str, _value: u8) -> Result<String, String> {
    Err("Audio endpoint switching is not available on Windows yet.".to_string())
}

// ---------------------------------------------------------------------------
// Process helper
// ---------------------------------------------------------------------------

async fn run(program: &str, args: &[&str], timeout_ms: u64) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let child = command
        .spawn()
        .map_err(|error| format!("{program}: {error}"))?;

    match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                Ok(stdout)
            } else if stderr.is_empty() {
                Err(format!("{program} failed: {stdout}"))
            } else {
                Err(stderr)
            }
        }
        Ok(Err(error)) => Err(format!("{program}: {error}")),
        Err(_) => Err(format!("{program} timed out.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_extracted() {
        assert_eq!(
            first_percent("Volume: front-left: 45875 /  70% / -9.29 dB"),
            Some(70)
        );
        assert_eq!(first_percent("Volume: front-left: 0 /   0% / -inf dB"), Some(0));
        assert_eq!(first_percent("Volume: none"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_pactl_sinks() {
        let text = "\
Sink #0
\tState: RUNNING
\tName: alsa_output.pci-0000_00_1f.3.analog-stereo
\tDescription: Built-in Audio Analog Stereo
\tMute: no
\tVolume: front-left: 45875 /  70% / -9.29 dB,   front-right: 45875 /  70% / -9.29 dB

Sink #1
\tName: bluez_output.aa_bb
\tDescription: WH-1000XM4
\tMute: yes
\tVolume: front-left: 65536 / 100% / 0.00 dB
";
        let devices = parse_sinks(text, "bluez_output.aa_bb");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].volume, 70);
        assert!(!devices[0].is_default);
        assert!(devices[1].muted);
        assert!(devices[1].is_default);
        assert_eq!(devices[1].description, "WH-1000XM4");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_pactl_sink_inputs() {
        let text = "\
Sink Input #42
\tSink: 0
\tMute: no
\tVolume: front-left: 32768 /  50% / -6.02 dB,   front-right: 32768 /  50% / -6.02 dB
\tProperties:
\t\tapplication.name = \"Firefox\"
\t\tmedia.name = \"Night Drive\"
";
        let apps = parse_sink_inputs(text);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "42");
        assert_eq!(apps[0].app, "Firefox");
        assert_eq!(apps[0].title, "Night Drive");
        assert_eq!(apps[0].volume, 50);
    }
}
