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
    /// Whether `devices[].volume` carries a real per-device reading. Windows
    /// reports endpoints without opening an audio session, so it is false there.
    pub supports_volume: bool,
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
        supports_volume: true,
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
            // `set-default-sink` only routes new streams; move the ones that are
            // already playing so the switch is audible straight away.
            let moved = move_streams(id).await;
            Ok(match moved {
                0 => "Output switched".to_string(),
                1 => "Output switched (1 stream moved)".to_string(),
                count => format!("Output switched ({count} streams moved)"),
            })
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
            run("pactl", &["set-sink-input-volume", &index, &level], 2_500).await?;
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

/// Moves every playing stream onto `sink` and reports how many were moved.
///
/// PulseAudio keeps existing sink-inputs on the old device when the default
/// sink changes, which makes a switch silent until each app restarts. Best
/// effort: a stream that disappears between listing and moving is skipped.
#[cfg(target_os = "linux")]
async fn move_streams(sink: &str) -> usize {
    let Ok(list) = run("pactl", &["list", "short", "sink-inputs"], 2_500).await else {
        return 0;
    };
    let mut moved = 0;
    for line in list.lines() {
        let Some(index) = line.split_whitespace().next() else {
            continue;
        };
        if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if run("pactl", &["move-sink-input", index, sink], 2_000)
            .await
            .is_ok()
        {
            moved += 1;
        }
    }
    moved
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
                supports_volume: false,
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
    win::snapshot()
}

#[cfg(target_os = "windows")]
async fn platform_apply(op: &str, id: &str, value: u8) -> Result<String, String> {
    match op {
        "output" => {
            if id.is_empty() {
                return Err("Select an output device first.".to_string());
            }
            let snapshot = win::snapshot();
            if !snapshot.devices.iter().any(|device| device.id == id) {
                return Err("Unknown output device. Refresh the mixer.".to_string());
            }
            win::set_default(id)?;
            Ok("Output switched".to_string())
        }
        "volume" | "mute" => {
            let _ = value;
            Err("Volume control is not available on Windows yet.".to_string())
        }
        other => Err(format!("Unknown audio operation: {other}")),
    }
}

/// Core Audio through COM: `IMMDeviceEnumerator` lists the active render
/// endpoints, and the undocumented `IPolicyConfig` interface changes the
/// default one — the two APIs Windows itself uses in the sound settings.
#[cfg(target_os = "windows")]
mod win {
    #![allow(non_snake_case)]

    use std::ffi::c_void;

    use windows::core::{interface, GUID, IUnknown, IUnknown_Vtbl, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::{
        eCommunications, eConsole, eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, DEVICE_STATE_ACTIVE, ERole,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
    };

    use super::{AudioDevice, AudioSnapshot};

    /// `IPolicyConfig` is not in the Windows SDK headers, but the interface has
    /// been stable since Windows 7. Only `SetDefaultEndpoint` is called; the
    /// methods before it exist so the vtable offsets line up.
    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(
            &self,
            device_id: PCWSTR,
            format: *mut *mut c_void,
        ) -> windows::core::Result<()>;
        fn GetDeviceFormat(
            &self,
            device_id: PCWSTR,
            is_default: i32,
            format: *mut *mut c_void,
        ) -> windows::core::Result<()>;
        fn ResetDeviceFormat(&self, device_id: PCWSTR) -> windows::core::Result<()>;
        fn SetDeviceFormat(
            &self,
            device_id: PCWSTR,
            endpoint_format: *mut c_void,
            mix_format: *mut c_void,
        ) -> windows::core::Result<()>;
        fn GetProcessingPeriod(
            &self,
            device_id: PCWSTR,
            is_default: i32,
            default_period: *mut i64,
            minimum_period: *mut i64,
        ) -> windows::core::Result<()>;
        fn SetProcessingPeriod(
            &self,
            device_id: PCWSTR,
            period: *mut i64,
        ) -> windows::core::Result<()>;
        fn GetShareMode(&self, device_id: PCWSTR, mode: *mut c_void) -> windows::core::Result<()>;
        fn SetShareMode(&self, device_id: PCWSTR, mode: *mut c_void) -> windows::core::Result<()>;
        fn GetPropertyValue(
            &self,
            device_id: PCWSTR,
            key: *const PROPERTYKEY,
            value: *mut c_void,
        ) -> windows::core::Result<()>;
        fn SetPropertyValue(
            &self,
            device_id: PCWSTR,
            key: *const PROPERTYKEY,
            value: *mut c_void,
        ) -> windows::core::Result<()>;
        fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> windows::core::Result<()>;
        fn SetEndpointVisibility(
            &self,
            device_id: PCWSTR,
            visible: i32,
        ) -> windows::core::Result<()>;
    }

    const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
    /// `PKEY_Device_FriendlyName`: {a45c254e-df1c-4efd-8020-67d146a850e0}, 14.
    const PKEY_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

    /// The server runs on a Tokio thread that has not touched COM yet. An
    /// already-initialised apartment (the tray or media session may have one)
    /// returns `RPC_E_CHANGED_MODE`, which is harmless here.
    fn ready() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
        ready();
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
    }

    fn endpoint_id(device: &IMMDevice) -> String {
        unsafe {
            match device.GetId() {
                Ok(id) => {
                    let text = id.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(id.0 as *const c_void));
                    text
                }
                Err(_) => String::new(),
            }
        }
    }

    fn friendly_name(device: &IMMDevice) -> String {
        unsafe {
            let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
                return String::new();
            };
            let Ok(value) = store.GetValue(&PKEY_FRIENDLY_NAME) else {
                return String::new();
            };
            let mut value: PROPVARIANT = value;
            let text = value
                .Anonymous
                .Anonymous
                .Anonymous
                .pwszVal
                .to_string()
                .unwrap_or_default();
            let _ = PropVariantClear(&mut value);
            text
        }
    }

    pub fn snapshot() -> AudioSnapshot {
        let Ok(enumerator) = enumerator() else {
            return AudioSnapshot {
                supported: false,
                backend: "WASAPI".to_string(),
                message: Some(
                    "Core Audio açılamadı. Windows ses servisi çalışıyor mu?".to_string(),
                ),
                ..AudioSnapshot::default()
            };
        };

        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .ok()
            .map(|device| endpoint_id(&device));

        let mut devices = Vec::new();
        if let Ok(collection) =
            unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        {
            let count = unsafe { collection.GetCount() }.unwrap_or(0);
            for index in 0..count {
                let Ok(device) = (unsafe { collection.Item(index) }) else {
                    continue;
                };
                let id = endpoint_id(&device);
                if id.is_empty() {
                    continue;
                }
                let name = friendly_name(&device);
                devices.push(AudioDevice {
                    is_default: default_id.as_deref() == Some(id.as_str()),
                    id,
                    description: if name.is_empty() {
                        "Ses çıkışı".to_string()
                    } else {
                        name
                    },
                    volume: 0,
                    muted: false,
                });
            }
        }

        AudioSnapshot {
            supported: true,
            backend: "WASAPI".to_string(),
            devices,
            apps: Vec::new(),
            supports_volume: false,
            message: None,
        }
    }

    pub fn set_default(id: &str) -> Result<(), String> {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        ready();
        let policy: IPolicyConfig =
            unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }
                .map_err(|error| format!("Ses ilkesi açılamadı: {error}"))?;
        for role in [eConsole, eMultimedia, eCommunications] {
            unsafe { policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role) }
                .map_err(|error| format!("Varsayılan çıkış değiştirilemedi: {error}"))?;
        }
        Ok(())
    }
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
        assert_eq!(
            first_percent("Volume: front-left: 0 /   0% / -inf dB"),
            Some(0)
        );
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
