//! Now-playing media snapshot for the panel.
//!
//! Windows uses the Global System Media Transport Controls (GSMTC) API, so the
//! panel mirrors whatever Spotify, a browser or any other player is doing
//! system-wide: title, artist, album, playback state, position and cover art.
//! Linux falls back to `playerctl`, macOS to `osascript`. When no platform
//! backend is available the snapshot simply stays empty.
//!
//! Polling runs on a dedicated thread because the WinRT async calls block; it
//! follows the same idle rule as the metric sampler: every second while a
//! panel is connected, every few seconds otherwise.

use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

/// Serialisable media snapshot embedded in every metrics sample.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MediaSnapshot {
    /// Friendly source application, e.g. `Spotify`.
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// True while the player reports the Playing state.
    pub playing: bool,
    /// Playback position in milliseconds.
    pub pos_ms: u64,
    /// Track length in milliseconds; 0 when the player exposes no timeline.
    pub dur_ms: u64,
    /// Whether the player accepts a seek request.
    pub can_seek: bool,
    /// Increments whenever a new cover image becomes available; 0 means none.
    pub art_version: u64,
}

/// Cover art bytes plus the MIME type reported by the player.
pub struct Art {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub version: u64,
}

/// Live media state shared with the HTTP handlers and the metrics sampler.
#[derive(Clone)]
pub struct MediaMonitor {
    snapshot: Arc<RwLock<Option<MediaSnapshot>>>,
    art: Arc<RwLock<Option<Arc<Art>>>>,
}

impl Default for MediaMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaMonitor {
    fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(None)),
            art: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the background poller. `clients` and `activity` decide how often
    /// the platform is asked; with no viewers the poll backs off.
    pub fn spawn(clients: Arc<AtomicUsize>, activity: Arc<AtomicU64>) -> Self {
        let monitor = Self::new();
        let sink = Sink {
            snapshot: monitor.snapshot.clone(),
            art: monitor.art.clone(),
        };

        let _ = std::thread::Builder::new()
            .name("media-monitor".to_string())
            .spawn(move || {
                let mut worker = platform::Worker::new();
                loop {
                    let active = clients.load(Ordering::Relaxed) > 0
                        || now_secs().saturating_sub(activity.load(Ordering::Relaxed)) < 15;

                    match worker.poll() {
                        Some(polled) => sink.publish(polled),
                        None => sink.clear(),
                    }

                    std::thread::sleep(Duration::from_millis(if active { 1_000 } else { 3_000 }));
                }
            });

        monitor
    }

    /// Latest snapshot, or `None` when nothing is playing.
    pub fn current(&self) -> Option<MediaSnapshot> {
        self.snapshot
            .read()
            .ok()
            .and_then(|snapshot| snapshot.clone())
    }

    /// Cover art for `version` (or the current cover when no version is given).
    pub fn art(&self, version: Option<u64>) -> Option<Arc<Art>> {
        let art = self.art.read().ok()?.clone()?;
        if version.is_some_and(|requested| requested != art.version) {
            return None;
        }
        Some(art)
    }
}

struct Sink {
    snapshot: Arc<RwLock<Option<MediaSnapshot>>>,
    art: Arc<RwLock<Option<Arc<Art>>>>,
}

impl Sink {
    fn publish(&self, polled: Polled) {
        if let Ok(mut slot) = self.art.write() {
            match polled.art {
                ArtUpdate::Keep => {}
                ArtUpdate::Clear => *slot = None,
                ArtUpdate::Set(data) => {
                    *slot = Some(Arc::new(Art {
                        bytes: data.bytes,
                        mime: data.mime,
                        version: polled.snapshot.art_version,
                    }));
                }
            }
        }
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = Some(polled.snapshot);
        }
    }

    fn clear(&self) {
        if let Ok(mut slot) = self.snapshot.write() {
            *slot = None;
        }
        if let Ok(mut slot) = self.art.write() {
            *slot = None;
        }
    }
}

/// One poll result: metadata plus what happened to the cover art.
struct Polled {
    snapshot: MediaSnapshot,
    art: ArtUpdate,
}

enum ArtUpdate {
    Keep,
    Clear,
    Set(ArtData),
}

struct ArtData {
    bytes: Vec<u8>,
    mime: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Maps a Windows application id to the name a person recognises.
#[cfg(windows)]
fn friendly_app(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    for (needle, name) in [
        ("spotify", "Spotify"),
        ("chrome", "Chrome"),
        ("helium", "Helium"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vivaldi", "Vivaldi"),
        ("firefox", "Firefox"),
        ("msedge", "Edge"),
        ("zune", "Media Player"),
        ("media player", "Media Player"),
        ("vlc", "VLC"),
        ("mpv", "mpv"),
        ("foobar", "foobar2000"),
        ("musicbee", "MusicBee"),
        ("aimp", "AIMP"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("amazon music", "Amazon Music"),
        ("youtube", "YouTube"),
    ] {
        if lower.contains(needle) {
            return name.to_string();
        }
    }
    let base = id.split('!').next().unwrap_or(id);
    let base = base.rsplit('\\').next().unwrap_or(base);
    let base = base
        .split('_')
        .next()
        .unwrap_or(base)
        .trim_end_matches(".exe");
    // Packaged apps end in a publisher hash: `Helium.WHPDYDLWKMQCHEBWE4MAYRLI2A`.
    let base = base.split('.').next().unwrap_or(base);
    if base.is_empty() {
        id.to_string()
    } else {
        base.to_string()
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };
    use windows::Storage::Streams::DataReader;

    /// Ticks between 1601-01-01 and 1970-01-01, in 100 ns units.
    const EPOCH_TICKS: i64 = 116_444_736_000_000_000;
    const MAX_ART_BYTES: u64 = 8 * 1024 * 1024;

    pub struct Worker {
        manager: Option<GlobalSystemMediaTransportControlsSessionManager>,
        track: String,
        art_version: u64,
        next_version: u64,
        art_attempts: u32,
        initialized: bool,
    }

    impl Worker {
        pub fn new() -> Self {
            Self {
                manager: None,
                track: String::new(),
                art_version: 0,
                next_version: 0,
                art_attempts: 0,
                initialized: false,
            }
        }

        pub fn poll(&mut self) -> Option<Polled> {
            if !self.initialized {
                self.initialized = init_com();
            }

            let manager = match self.manager() {
                Some(manager) => manager,
                None => {
                    self.reset_track();
                    return None;
                }
            };

            let session = match manager.GetCurrentSession() {
                Ok(session) => session,
                Err(_) => {
                    self.reset_track();
                    return None;
                }
            };

            let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
            let title = props
                .Title()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let artist = props
                .Artist()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let album = props
                .AlbumTitle()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let app = session
                .SourceAppUserModelId()
                .map(|value| friendly_app(&value.to_string()))
                .unwrap_or_default();

            let playback = session.GetPlaybackInfo().ok();
            let playing = playback
                .as_ref()
                .and_then(|info| info.PlaybackStatus().ok())
                .is_some_and(|status| {
                    status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                });
            let can_seek = playback
                .as_ref()
                .and_then(|info| info.Controls().ok())
                .and_then(|controls| controls.IsPlaybackPositionEnabled().ok())
                .unwrap_or(false);

            let (pos_ms, dur_ms) = timeline(&session, playing);

            let key = format!("{app}|{title}|{artist}");
            let mut art = ArtUpdate::Keep;
            if key != self.track {
                self.track = key;
                self.art_attempts = 0;
                self.art_version = 0;
                art = ArtUpdate::Clear;

                // Some players hand the cover over a moment after the metadata.
                if self.art_attempts < 8 {
                    self.art_attempts += 1;
                    if let Some(data) = fetch_art(&props) {
                        self.next_version = self.next_version.wrapping_add(1);
                        self.art_version = self.next_version;
                        art = ArtUpdate::Set(data);
                    }
                }
            } else if self.art_version == 0 && self.art_attempts < 8 {
                self.art_attempts += 1;
                if let Some(data) = fetch_art(&props) {
                    self.next_version = self.next_version.wrapping_add(1);
                    self.art_version = self.next_version;
                    art = ArtUpdate::Set(data);
                }
            }

            Some(Polled {
                snapshot: MediaSnapshot {
                    app,
                    title,
                    artist,
                    album,
                    playing,
                    pos_ms,
                    dur_ms,
                    can_seek,
                    art_version: self.art_version,
                },
                art,
            })
        }

        fn manager(&mut self) -> Option<&GlobalSystemMediaTransportControlsSessionManager> {
            if self.manager.is_none() {
                self.manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                    .ok()
                    .and_then(|operation| operation.join().ok());
            }
            self.manager.as_ref()
        }

        fn reset_track(&mut self) {
            self.track.clear();
            self.art_version = 0;
            self.art_attempts = 0;
        }
    }

    fn init_com() -> bool {
        use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        // A fresh worker thread: MTA avoids the message pump an STA would need.
        let result = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
        result >= 0
    }

    fn timeline(
        session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession,
        playing: bool,
    ) -> (u64, u64) {
        let Ok(timeline) = session.GetTimelineProperties() else {
            return (0, 0);
        };
        let start = timeline
            .StartTime()
            .map(|value| value.Duration)
            .unwrap_or(0);
        let end = timeline.EndTime().map(|value| value.Duration).unwrap_or(0);
        let mut position = timeline.Position().map(|value| value.Duration).unwrap_or(0);
        let duration = (end - start).max(0);

        if playing {
            if let Ok(updated) = timeline.LastUpdatedTime() {
                let now = now_ticks();
                let delta = now - updated.UniversalTime;
                if delta > 0 && delta < 10_000 * 60 * 60 {
                    position += delta;
                }
            }
        }

        let position = position.clamp(0, duration.max(0)) / 10_000;
        (position as u64, (duration / 10_000) as u64)
    }

    fn now_ticks() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| {
                EPOCH_TICKS
                    + duration.as_secs() as i64 * 10_000_000
                    + (duration.subsec_nanos() as i64 / 100)
            })
            .unwrap_or(EPOCH_TICKS)
    }

    fn fetch_art(
        props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
    ) -> Option<ArtData> {
        let thumbnail = props.Thumbnail().ok()?;
        let stream = thumbnail.OpenReadAsync().ok()?.join().ok()?;
        let size = stream.Size().ok()?;
        if size == 0 || size > MAX_ART_BYTES {
            return None;
        }

        let mime = stream
            .ContentType()
            .map(|value| value.to_string())
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "image/jpeg".to_string());

        let reader = DataReader::CreateDataReader(&stream).ok()?;
        let mut bytes = Vec::with_capacity(size as usize);
        let mut remaining = size as u32;
        while remaining > 0 {
            let loaded = reader
                .LoadAsync(remaining.min(64 * 1024))
                .ok()?
                .join()
                .ok()?;
            if loaded == 0 {
                break;
            }
            let start = bytes.len();
            bytes.resize(start + loaded as usize, 0);
            reader.ReadBytes(&mut bytes[start..]).ok()?;
            remaining = remaining.saturating_sub(loaded);
        }

        if bytes.is_empty() {
            None
        } else {
            Some(ArtData { bytes, mime })
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub struct Worker;

    impl Worker {
        pub fn new() -> Self {
            Self
        }

        pub fn poll(&mut self) -> Option<Polled> {
            let title = playerctl(&["metadata", "title"])?;
            if title.is_empty() {
                return None;
            }
            let artist = playerctl(&["metadata", "artist"]).unwrap_or_default();
            let album = playerctl(&["metadata", "album"]).unwrap_or_default();
            let app = playerctl(&["-l"])
                .and_then(|list| {
                    list.lines()
                        .next()
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "Media".to_string());
            let playing = playerctl(&["status"])
                .map(|status| status == "Playing")
                .unwrap_or(false);
            let duration = playerctl(&["metadata", "mpris:length"])
                .and_then(|value| value.parse::<u64>().ok())
                .map(|micros| micros / 1_000)
                .unwrap_or(0);
            let position = playerctl(&["position"])
                .and_then(|value| value.parse::<f64>().ok())
                .map(|seconds| (seconds.max(0.0) * 1_000.0) as u64)
                .unwrap_or(0);

            Some(Polled {
                snapshot: MediaSnapshot {
                    app,
                    title,
                    artist,
                    album,
                    playing,
                    pos_ms: position.min(duration),
                    dur_ms: duration,
                    can_seek: false,
                    art_version: 0,
                },
                art: ArtUpdate::Clear,
            })
        }
    }

    fn playerctl(args: &[&str]) -> Option<String> {
        let output = std::process::Command::new("playerctl")
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub struct Worker;

    impl Worker {
        pub fn new() -> Self {
            Self
        }

        pub fn poll(&mut self) -> Option<Polled> {
            for app in ["Spotify", "Music"] {
                if let Some(snapshot) = query(app) {
                    return Some(Polled {
                        snapshot,
                        art: ArtUpdate::Clear,
                    });
                }
            }
            None
        }
    }

    fn query(app: &str) -> Option<MediaSnapshot> {
        let script = format!(
            r#"tell application "{app}"
if it is running then
return (name of current track) & "\n" & (artist of current track) & "\n" & (album of current track) & "\n" & (player state as text) & "\n" & (duration of current track) & "\n" & (player position)
end if
return ""
end tell"#
        );
        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = text.trim().split('\n');
        let title = lines.next()?.trim().to_string();
        if title.is_empty() {
            return None;
        }
        let artist = lines.next().unwrap_or("").trim().to_string();
        let album = lines.next().unwrap_or("").trim().to_string();
        let playing = lines
            .next()
            .map(|value| value.trim() == "playing")
            .unwrap_or(false);
        let duration = lines
            .next()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1_000))
            .unwrap_or(0);
        let position = lines
            .next()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|seconds| (seconds.max(0.0) * 1_000.0) as u64)
            .unwrap_or(0);

        Some(MediaSnapshot {
            app: app.to_string(),
            title,
            artist,
            album,
            playing,
            pos_ms: position.min(duration),
            dur_ms: duration,
            can_seek: false,
            art_version: 0,
        })
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    pub struct Worker;

    impl Worker {
        pub fn new() -> Self {
            Self
        }

        pub fn poll(&mut self) -> Option<Polled> {
            None
        }
    }
}
