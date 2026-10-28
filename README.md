# Liquid Deck

<img src="assets/icon.png" width="112" align="right" alt="Liquid Deck icon">

**A single-binary, cross-platform Stream Deck and live hardware dashboard with an iOS Liquid Glass web UI.**

Liquid Deck runs on the machine you want to monitor, serves an embedded dashboard over HTTP and WebSocket, and turns a phone or tablet — including a Galaxy Note 4 — into an always-on hardware panel and macro deck. There is no Electron, no Node runtime, no external font or asset CDN, and no database. The release binary is about **1.4 MB** and idles at roughly **3 MB RSS**.

- Live CPU, RAM, temperature, network throughput, TCP ping, load average and uptime.
- A configurable, multi-board touch deck with a full in-browser editor, nested boards, persistent toggle states, URL launchers, OBS WebSocket v5, a soundboard and a system audio mixer.
- Discord mute/deafen, media keys, volume, terminal, game mode and custom scripts.
- AMOLED-black, blurred-glass UI with specular edges, squircle cards and a 1440 × 2560-friendly layout.
- WebSocket telemetry with automatic reconnect and an HTTP polling fallback.
- Optional token authentication, constant-time comparison, a 25 action/second rate limit and a strict action allow-list.
- Linux, Windows and macOS builds from one Rust codebase.

---

## Contents

- [Architecture](#architecture)
- [Requirements](#requirements)
- [Build](#build)
- [Run](#run)
- [Configuration](#configuration)
- [Actions](#actions)
- [HTTP and WebSocket API](#http-and-websocket-api)
- [Galaxy Note 4 panel over ADB](#galaxy-note-4-panel-over-adb)
- [Deployment](#deployment)
- [Security](#security)
- [CI and release artifacts](#ci-and-release-artifacts)
- [Performance](#performance)
- [Troubleshooting](#troubleshooting)
- [Project layout](#project-layout)
- [License](#license)

---

## Architecture

```text
┌──────────────────────────────────────────────────────────────────────┐
│  Browser: Galaxy Note 4, phone, tablet or desktop                    │
│  web/index.html · web/style.css · web/app.js                         │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ HTTP + WebSocket (same origin)
┌───────────────────────────────▼──────────────────────────────────────┐
│  liquid-deck (single Rust binary, one current-thread Tokio runtime)  │
│                                                                      │
│  axum router                                                         │
│   ├── /                embedded static UI (rust-embed)               │
│   ├── /ws              hello / metrics / config / state / actions    │
│   ├── /api/health      public liveness + client count                │
│   ├── /api/auth        token check                                   │
│   ├── /api/metrics     latest sample (polling fallback)              │
│   ├── /api/actions     action catalog                                │
│   ├── /api/action      authenticated action execution                │
│   ├── /api/config      board layout read / write / reset             │
│   ├── /api/state       toggle + active-board persistence             │
│   ├── /api/audio       mixer snapshot and level changes              │
│   ├── /api/sounds      sound library and clip streaming              │
│   └── /api/obs/test    obs-websocket connection check                │
│                                                                      │
│  metrics sampler ── sysinfo ── CPU, RAM, components, networks        │
│  ping task ──────── TCP connect to LIQUID_DECK_PING_TARGET           │
│  action executor ── xdotool / wtype / playerctl / pactl / PowerShell  │
│                     / osascript / launch commands / scripts / OBS    │
│  deck store ─────── boards.json + state.json (atomic writes)         │
└──────────────────────────────────────────────────────────────────────┘
```

### Backend

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI/env configuration, axum routes, WebSocket loop, token auth, rate limiting, security headers, static asset serving, graceful shutdown. |
| `src/metrics.rs` | Adaptive `sysinfo` sampler, 60-point history ring, network rate calculation, hottest thermal sensor, TCP ping task. |
| `src/actions.rs` | Action enum, validation, platform backends, script confinement, process timeouts, unit tests. |
| `src/deck.rs` | `boards.json` model, defaults, normalisation/validation, atomic persistence, `state.json` toggle store. |
| `src/audio.rs` | Audio output/app mixer via `pactl` on Linux, `SwitchAudioSource` + AppleScript on macOS. |
| `src/obs.rs` | Minimal obs-websocket v5 client: hello, SHA-256 challenge auth, request/response, allow-listed operations. |
| `src/sounds.rs` | Sound library discovery, path confinement, built-in clips and platform playback. |
| `web/index.html` | Dashboard markup, board switcher, editor/mixer sheets, SVG icon sprite, accessible controls. |
| `web/style.css` | Liquid Glass design system, AMOLED palette, ambient glow, board/editor/mixer sheets, responsive layout. |
| `web/app.js` | WebSocket client, reconnect/backoff, polling fallback, gauges, configurable boards, action deck, editor, mixer, toasts, lock screen. |
| `web/icon.svg`, `web/icon-*.png`, `web/noise.png` | Embedded icons and grain texture. |
| `web/manifest.webmanifest` | Installable PWA metadata. |
| `integrations/note4-panel.sh` | Optional ADB helper for waking/sleeping a Galaxy Note 4 panel. |

### Metrics flow

1. The sampler refreshes CPU, memory, components and networks with `sysinfo`.
2. Network counters are converted to bytes/second using the elapsed time between samples. Loopback interfaces are ignored.
3. A TCP handshake to `1.1.1.1:443` (configurable) measures latency without requiring root or raw sockets.
4. The latest sample is stored in an `RwLock`; the last 60 samples are kept in a `VecDeque` for the network chart.
5. While at least one WebSocket viewer is connected, a sample is broadcast every `LIQUID_DECK_INTERVAL_MS` (default 1000 ms).
6. With no viewers and no recent HTTP poll, the sampler sleeps for `LIQUID_DECK_IDLE_MS` (default 5000 ms) and **skips all sysinfo refreshes**. An idle deck costs effectively no CPU.
7. The ping task runs every 5 seconds while active and also skips probes when idle.

### Frontend flow

- On load, the UI reads `/api/health`, restores a token from `localStorage` or `?token=`, and verifies it with `/api/auth` when authentication is enabled. It also fetches `/api/config` so the deck is usable before the WebSocket connects.
- It opens `ws://host/ws` (or `wss://` on HTTPS). The first message is `hello`, containing the server version, host, OS, action catalog, current history, the full deck configuration, toggle state and sound library.
- Every `metrics` message updates the CPU/RAM rings, network rates, SVG chart, ping pill, clock, load and uptime.
- Board tabs switch pages without reloading. Long-press a key (or use the sliders button) to open the editor; edits are staged in a draft and saved with `PUT /api/config`.
- Toggle buttons persist on/off state in `state.json` through `POST /api/state`. Configuration and state changes are broadcast to every connected client.
- If the WebSocket closes, the client reconnects with exponential backoff (450 ms → 10 s, plus jitter) and polls `/api/metrics` every 2 seconds in the meantime.
- Action buttons send an `action` message with a `request_id` and wait for `action_result`; if the WebSocket is unavailable, they fall back to `POST /api/action`.
- A 401 locks the UI behind a token prompt. The token is stored under the `liquid-deck.token` key.

---

## Requirements

### All platforms

- Rust **1.85 or newer** (stable). Install from [rustup.rs](https://rustup.rs/).
- A C linker/toolchain for your platform (see below).
- `curl` is useful for testing the API, but is not required at runtime.

### Linux

Build dependencies:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config
```

Optional runtime tools, depending on which actions you use:

| Feature | Package / tool |
| --- | --- |
| X11 key and media injection | `xdotool` |
| Wayland key injection | `wtype` |
| Media keys / MPRIS | `playerctl` |
| Volume (PipeWire/PulseAudio) | `pactl` (`pulseaudio-utils`) or `wpctl` (`wireplumber`) |
| Volume (ALSA fallback) | `amixer` (`alsa-utils`) |
| Audio mixer | `pactl` (`pulseaudio-utils`, PipeWire's PulseAudio shim also provides it) |
| Soundboard playback | one of `paplay`, `pw-play`, `ffplay`, `mpv`, `aplay` |
| Open browser/files | `xdg-open` (`xdg-utils`) |
| Terminal action | any of `x-terminal-emulator`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `kitty`, `alacritty`, `foot`, `xterm` |
| OBS WebSocket v5 | OBS Studio 28+ with **Tools → WebSocket Server Settings** enabled |
| Temperature sensors | kernel `hwmon` drivers; `lm-sensors` helps expose them |

Wayland compositors may block synthetic input. `wtype` works on wlroots-based compositors; GNOME/KDE may require additional permissions or extensions.

### Windows

- **Visual Studio Build Tools** with the “Desktop development with C++” workload (MSVC linker).
- PowerShell 5.1+ (built in) for key, media and volume actions.
- No extra runtime dependencies.

### macOS

- Xcode Command Line Tools: `xcode-select --install`.
- Grant **Accessibility** permission to the terminal app (or to the `liquid-deck` binary) in **System Settings → Privacy & Security → Accessibility**, otherwise `osascript` key events are ignored.
- Media actions try Apple Music and Spotify via AppleScript.
- The audio mixer uses `SwitchAudioSource` for output switching (`brew install switchaudio-osx`) and AppleScript for level/mute changes.
- Soundboard clips play through `afplay`.

---

## Build

```bash
git clone https://github.com/serkanagdas/liquid-deck.git
cd liquid-deck

# development build
cargo build

# optimised, stripped release binary
cargo build --release
```

The binary is written to:

| Platform | Path |
| --- | --- |
| Linux/macOS | `target/release/liquid-deck` |
| Windows | `target\release\liquid-deck.exe` |

The release profile uses `opt-level = "z"`, LTO, one codegen unit, `panic = "abort"` and symbol stripping to keep the binary small.

### Tests and lints

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

`Cargo.lock` is committed, so `--locked` is reproducible.

### Static Linux binary (musl)

```bash
rustup target add x86_64-unknown-linux-musl
sudo apt install -y musl-tools
cargo build --release --locked --target x86_64-unknown-linux-musl
```

The result runs on most x86_64 Linux distributions without a matching glibc.

### AppImage

The GitHub Actions workflow builds `LiquidDeck-x86_64.AppImage` automatically. To build one locally:

```bash
cargo build --release --locked --target x86_64-unknown-linux-musl

APPDIR="$PWD/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/512x512/apps"

cp target/x86_64-unknown-linux-musl/release/liquid-deck "$APPDIR/usr/bin/liquid-deck"
cp web/icon-512.png "$APPDIR/liquid-deck.png"
cp web/icon-512.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/liquid-deck.png"

cat > "$APPDIR/liquid-deck.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Liquid Deck
Comment=Stream Deck and live hardware dashboard
Exec=liquid-deck
Icon=liquid-deck
Terminal=true
Categories=Utility;System;
EOF
cp "$APPDIR/liquid-deck.desktop" "$APPDIR/usr/share/applications/liquid-deck.desktop"

curl -fsSL -o appimagetool \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x appimagetool
ARCH=x86_64 ./appimagetool --appimage-extract-and-run "$APPDIR" LiquidDeck-x86_64.AppImage
```

If FUSE is unavailable, run the AppImage with `--appimage-extract-and-run`.

### Cross-compilation notes

- Linux → Windows and Linux → macOS are not supported by the CI; use the matrix runners.
- The workflow builds `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`, `aarch64-apple-darwin` and `x86_64-apple-darwin`.
- macOS builds are unsigned. On first launch, use **Right-click → Open**, or clear the quarantine flag:

```bash
xattr -d com.apple.quarantine ./liquid-deck
```

- Windows SmartScreen may warn about an unsigned executable; choose **More info → Run anyway**.

---

## Run

```bash
# local only (default)
./target/release/liquid-deck

# reachable from the Note 4 / LAN, with a token
./target/release/liquid-deck --lan --token "$(openssl rand -hex 16)"

# custom port, scripts enabled
./target/release/liquid-deck --lan --port 9000 --token my-secret --scripts ./scripts

# custom board layout + soundboard directory
./target/release/liquid-deck --lan --token my-secret \
  --config ./boards.json --sounds ~/Music/clips
```

The default data directory is `$XDG_CONFIG_HOME/liquid-deck` (usually `~/.config/liquid-deck`) on Linux/macOS and `%APPDATA%\liquid-deck` on Windows. It contains `boards.json`, `state.json` and a `sounds/` folder seeded with three built-in clips on first run. Use `--data <DIR>` to move all three, or `--no-sounds` to disable the soundboard.

Open `http://127.0.0.1:8787/` on the host, or `http://<host-lan-ip>:8787/` from the tablet.

On Windows PowerShell:

```powershell
$env:LIQUID_DECK_TOKEN = "my-secret"
.\liquid-deck.exe --lan
```

On macOS/Linux with a shell:

```bash
LIQUID_DECK_TOKEN=my-secret ./liquid-deck --lan
```

The startup banner prints the local URL, whether authentication is enabled, the scripts and sounds directories, the board layout path, and the sampling/ping configuration.

### Token in the URL

For a browser, the simplest first visit is:

```text
http://192.168.1.50:8787/?token=my-secret
```

`app.js` stores the token in `localStorage` and removes it from the address bar with `history.replaceState`. Later visits do not need the query parameter. If the token is rotated, clear the `liquid-deck.token` key in the browser’s site data or use the lock screen.

---

## Configuration

Command-line flags override environment variables, which override defaults.

| Flag | Environment variable | Default | Description |
| --- | --- | --- | --- |
| `--bind <ADDR>` | `LIQUID_DECK_BIND` | `127.0.0.1` | Bind IP or `IP:PORT`. |
| `--port <PORT>` | `LIQUID_DECK_PORT` | `8787` | Port used when `--bind` does not contain one. |
| `--lan` | — | off | Shortcut for `--bind 0.0.0.0`. |
| `--local` | — | on | Shortcut for `--bind 127.0.0.1`. |
| `--token <TOKEN>` | `LIQUID_DECK_TOKEN` | none | Require a token for metrics, WebSocket and actions. Empty means disabled. |
| `--scripts <DIR>` | `LIQUID_DECK_SCRIPTS` | disabled | Enable `script` actions from this directory. |
| `--config <FILE>` | `LIQUID_DECK_CONFIG` | `<data>/boards.json` | Board layout file. |
| `--data <DIR>` | `LIQUID_DECK_DATA` | platform config dir | Data directory for config, state and sounds. |
| `--sounds <DIR>` | `LIQUID_DECK_SOUNDS` | `<data>/sounds` | Soundboard directory. |
| `--no-sounds` | — | off | Disable the soundboard. |
| `--ping <HOST:PORT>` | `LIQUID_DECK_PING_TARGET` | `1.1.1.1:443` | TCP ping target. A bare host gets `:443`. |
| — | `LIQUID_DECK_INTERVAL_MS` | `1000` | Active sample interval, clamped to 250–10000 ms. |
| — | `LIQUID_DECK_IDLE_MS` | `5000` | Idle sample interval, clamped to 1000–60000 ms. |
| — | `LIQUID_DECK_LAUNCH_<TARGET>` | platform default | Override a launch target with a custom command. |
| — | `LIQUID_DECK_GAME_MODE_ON` | no hook | Shell command run when game mode is enabled. |
| — | `LIQUID_DECK_GAME_MODE_OFF` | no hook | Shell command run when game mode is disabled. |
| `-h`, `--help` | — | — | Show help. |
| `-V`, `--version` | — | — | Show the version. |

Notes:

- `--bind 192.168.1.50:9000` uses port 9000 and ignores `--port`.
- `--bind 192.168.1.50` uses `--port` / `LIQUID_DECK_PORT`.
- `--lan` exposes the server to every interface. Use a token on any network you do not fully trust.
- The server has **no TLS**. Put it behind a reverse proxy if it leaves a trusted LAN.

### Boards, editor and state

The deck is stored in `<data>/boards.json`. On first run the server writes the built-in **Gaming**, **Workstation**, **Media** and **Dev** boards. Limits: 16 boards, 32 buttons per board, 2–8 columns, button width 1–4 columns.

```json
{
  "version": 1,
  "settings": {
    "default_board": "gaming",
    "haptics": true,
    "show_subtitles": true,
    "obs": { "url": "ws://127.0.0.1:4455", "password": "" }
  },
  "boards": [
    {
      "id": "gaming",
      "name": "Gaming",
      "icon": "gamepad",
      "accent": "#ff9f0a",
      "columns": 4,
      "buttons": [
        {
          "id": "mic",
          "label": "Mic Mute",
          "subtitle": "Discord · Ctrl ⇧ M",
          "icon": "mic",
          "active_label": "Muted",
          "active_icon": "mic-off",
          "color": "orange",
          "action": "discord_mute",
          "value": "",
          "arg": "",
          "amount": 5,
          "toggle": true,
          "span": 1
        }
      ]
    }
  ]
}
```

- Open the editor with the sliders button beside the board tabs, or long-press any key. It has **Boards**, **Buttons**, **Integrations** and **JSON** tabs. Changes are staged and saved with `PUT /api/config`.
- `color` / `accent` accept a preset (`blue`, `cyan`, `violet`, `pink`, `green`, `orange`, `yellow`, `red`, `slate`) or `#rrggbb`.
- `icon` accepts a sprite name (for example `mic`, `record`, `speaker`, `globe`) or a short text/emoji glyph.
- A button with `"action": "board"` and a board id in `value` opens a nested board. The UI shows a **Back** pill until you tap a tab.
- `"action": "panel"` with `value` set to `editor`, `settings` or `audio` opens a sheet without leaving the deck.
- `"toggle": true` keeps an on/off state. Toggle state lives in `<data>/state.json` (keyed as `board_id/button_id`) and survives reloads and layout saves.
- Unknown actions, broken board links, invalid URLs, unsafe sound names and non-`ws://` OBS URLs are rejected with a useful error. Labels, colours and widths are normalised instead of failing.
- `POST /api/config/reset` restores the built-in deck and clears all toggle state.

Writes are atomic: the server writes a temporary file, sets mode `0600` on Unix and renames it into place.

### Launch targets

`launch` accepts `terminal`, `browser`, `files`, `calculator` and `editor`. Override any of them:

```bash
LIQUID_DECK_LAUNCH_TERMINAL='kitty --directory ~' \
LIQUID_DECK_LAUNCH_EDITOR='code ~/projects' \
./liquid-deck --lan
```

On Windows PowerShell:

```powershell
$env:LIQUID_DECK_LAUNCH_TERMINAL = "wt.exe"
$env:LIQUID_DECK_LAUNCH_EDITOR = "code.exe"
```

### Game mode hooks

Game mode is a pair of administrator-defined shell commands. The UI toggles the state locally and sends `enabled: true/false`.

```bash
LIQUID_DECK_GAME_MODE_ON='notify-send "Game mode on"; pkill -STOP discord' \
LIQUID_DECK_GAME_MODE_OFF='pkill -CONT discord' \
./liquid-deck --lan
```

Hooks run through the platform shell with an 8-second timeout. They are configuration, not network input: never build them from a request body.

---

## Actions

`GET /api/actions` and the WebSocket `hello` message expose this catalog:

```json
["key","media","volume","mute","launch","script","discord_mute","discord_deafen","game_mode","terminal","url","sound","obs","audio"]
```

`board`, `panel` and `none` are frontend-only configuration values: they never reach the action executor.

| UI key | Request body |
| --- | --- |
| Mic Mute | `{"action":"discord_mute"}` |
| Deafen | `{"action":"discord_deafen"}` |
| Play / Pause | `{"action":"media","command":"play_pause"}` |
| Next Track | `{"action":"media","command":"next"}` |
| Volume Down | `{"action":"volume","direction":"down","step":5}` |
| Volume Up | `{"action":"volume","direction":"up","step":5}` |
| Terminal | `{"action":"terminal"}` |
| Game Mode | `{"action":"game_mode","enabled":true}` |
| Open URL | `{"action":"url","url":"https://example.com"}` |
| Sound clip | `{"action":"sound","name":"click.wav"}` |
| OBS record | `{"action":"obs","op":"record_toggle","target":""}` |
| OBS scene | `{"action":"obs","op":"scene","target":"Main"}` |
| Switch output | `{"action":"audio","op":"output","id":"alsa_output.…"}` |

### `key`

```json
{"action":"key","combo":"ctrl+shift+m"}
```

- Modifiers: `ctrl`/`control`, `shift`, `alt`/`option`, `super`/`meta`/`win`/`cmd`.
- Key: one ASCII letter or digit, or a named key such as `enter`, `escape`, `tab`, `space`, `backspace`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `print`, `f1`–`f12`.
- Punctuation and arbitrary text are rejected. The combo is limited to 48 characters and four modifiers.
- Linux: `xdotool` first, then `wtype` (Wayland).
- macOS: `osascript` / System Events. Accessibility permission is required.
- Windows: PowerShell `WScript.Shell.SendKeys`.

### `media`

Accepted commands: `play_pause` (`playpause`, `toggle`), `next`, `previous`, `stop`.

- Linux: `playerctl`, then `xdotool` media keys.
- macOS: Apple Music, then Spotify.
- Windows: `keybd_event` media virtual keys.

### `volume`

```json
{"action":"volume","direction":"up","step":5}
```

Directions: `up`, `down`, `mute`, `toggle`. `step` is clamped to 1–50 and defaults to 5.

- Linux: `pactl`, then `wpctl`, then `amixer`.
- macOS: AppleScript `output volume`.
- Windows: `keybd_event` volume keys. Windows steps in 2% increments, so `step: 5` performs two presses (about 4%).

### `mute`

Alias for `{"action":"volume","direction":"mute"}`.

### `launch`

Targets: `terminal`, `browser`, `files`, `calculator`, `editor`. `terminal` has a dedicated alias:

```json
{"action":"terminal"}
```

### `script`

```json
{"action":"script","name":"morning.sh"}
```

Requirements:

- Start the server with `--scripts <DIR>` / `LIQUID_DECK_SCRIPTS`.
- The name must be a relative path using only letters, digits, `_`, `-`, `.` and `/`.
- `..`, absolute paths and NUL bytes are rejected.
- The resolved path must canonicalize inside the scripts directory.
- The target must be a regular file.
- `.sh` files are run with `sh` if not executable; `.py` files with `python3`; on Windows `.ps1` uses PowerShell, `.bat`/`.cmd` use `cmd.exe`, `.py` uses `python.exe`.
- Other extensions must be executable.
- Output is captured and returned in `message`; successful output is truncated to 1200 characters, aggregated error text to 400. The process timeout is 10 seconds.

### `game_mode`

```json
{"action":"game_mode","enabled":true}
```

Runs `LIQUID_DECK_GAME_MODE_ON` or `LIQUID_DECK_GAME_MODE_OFF` if configured. Without a hook it succeeds with “no hook configured”.

### `discord_mute` / `discord_deafen`

Convenience wrappers for Discord’s default shortcuts `Ctrl+Shift+M` and `Ctrl+Shift+D`. They use the same platform key backend as `key`.

### `url`

```json
{"action":"url","url":"https://discord.com/app"}
```

Only `http://` and `https://` URLs up to 2048 characters are accepted. Whitespace, control characters and every other scheme (`file:`, `javascript:`…) are rejected. The URL opens in the host’s default browser with `xdg-open` (Linux), `open` (macOS) or `cmd /C start` (Windows).

### `sound`

```json
{"action":"sound","name":"click.wav"}
```

- Play clips from the soundboard directory (`--sounds <DIR>` / `LIQUID_DECK_SOUNDS`, default `<data>/sounds`).
- Supported extensions: `wav`, `mp3`, `ogg`, `oga`, `flac`, `m4a`, `aac`, `opus`, `aiff`, `aif`; maximum 32 MB.
- Names may not contain `/`, `\` or `..`, and the resolved path must stay inside the soundboard directory.
- First run seeds `click.wav`, `chime.wav` and `alert.wav`.
- Linux tries `paplay`, `pw-play`, `ffplay`, `mpv`, then `aplay`. macOS uses `afplay`; Windows uses PowerShell’s `MediaPlayer`.

`GET /api/sound?name=…` streams a clip for browser preview; it is authenticated like every other API route.

### `obs`

```json
{"action":"obs","op":"scene","target":"Main"}
```

Each action opens one short-lived obs-websocket v5 session, authenticates with the configured password, sends one request and closes. Only `ws://` is supported directly; for `wss://` put a TLS proxy in front of OBS.

Allowed operations: `scene`, `record_start`, `record_stop`, `record_toggle`, `stream_start`, `stream_stop`, `stream_toggle`, `mute_toggle`, `mute_on`, `mute_off`, `replay_start`, `replay_stop`, `replay_save`, `scene_collection`, `test`. `target` is the scene name or input/source name where relevant.

Configure the server URL and password in **Customize → Integrations**, then use **Test connection**. The password is stored in `boards.json` with the same `0600` permissions as the rest of the file and is never logged.

### `audio`

```json
{"action":"audio","op":"volume","id":"alsa_output.pci-0000_00_1f.3.analog-stereo","value":45}
```

Allowed operations:

| `op` | `id` | `value` |
| --- | --- | --- |
| `output` | sink name | ignored |
| `volume` | sink name, or empty for default | 0–100 |
| `mute` | sink name, or empty for default | ignored |
| `app_volume` | numeric sink-input index | 0–100 |
| `app_mute` | numeric sink-input index | ignored |

Linux uses `pactl`. macOS uses `SwitchAudioSource` for output switching and AppleScript for volume/mute. Windows reports that the mixer is unsupported. The mixer sheet beside the board tabs lists device and application ids that can be pasted into a button.

### `board` / `panel`

These are handled entirely by `app.js` and are never sent to the server:

```json
{"action":"board","value":"media"}
{"action":"panel","value":"audio"}
```

`board` navigates to another board and remembers the previous one for the **Back** pill. `panel` accepts `editor`, `settings` or `audio`.

### Action response

```json
{
  "ok": true,
  "action": "media",
  "message": "Media: play-pause",
  "duration_ms": 12
}
```

Failures return `ok: false` with a human-readable `message`. Over WebSocket the same object is wrapped:

```json
{
  "type": "action_result",
  "request_id": "6f1c…",
  "ok": true,
  "action": "volume",
  "message": "Volume up 5%",
  "duration_ms": 8
}
```

---

## HTTP and WebSocket API

All JSON endpoints return `application/json`. Static files are served from the embedded `web/` directory with ETags and a strict Content-Security-Policy.

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `GET` | `/` | public | Dashboard UI. |
| `GET` | `/api/health` | public | Liveness, version, host, OS, client count, feature flags. |
| `GET` | `/api/auth` | token | Validate a token. |
| `GET` | `/api/metrics` | token | Latest metrics sample, with `"type":"metrics"`. |
| `GET` | `/api/actions` | public | Action catalog. |
| `POST` | `/api/action` | token | Execute one action. |
| `GET` | `/api/config` | token | Deck layout, toggle state and sound library. |
| `PUT` | `/api/config` | token | Validate, normalise, persist and broadcast a new layout. |
| `POST` | `/api/config/reset` | token | Restore the built-in deck and clear toggle state. |
| `POST` | `/api/state` | token | Set one toggle, or the active board. |
| `GET` | `/api/audio` | token | Mixer snapshot (outputs + applications). |
| `POST` | `/api/audio` | token | Change output, volume or mute. |
| `GET` | `/api/sounds` | token | Soundboard library. |
| `GET` | `/api/sound?name=…` | token | Stream one clip. |
| `POST` | `/api/obs/test` | token | Test an obs-websocket connection. |
| `GET` | `/ws` | token | WebSocket telemetry, deck events and action channel. |

Authentication accepts either:

```http
x-liquid-token: my-secret
```

or:

```http
Authorization: Bearer my-secret
```

or `?token=my-secret` for browser WebSocket handshakes. The token is compared in constant time. When no token is configured, everything is open — fine for `127.0.0.1`, not for a LAN.

### `GET /api/health`

```json
{
  "auth_required": false,
  "clients": 1,
  "host": "desk",
  "ok": true,
  "os": "Linux (Ubuntu 24.04)",
  "sample_ms": 1000,
  "scripts": false,
  "uptime": 268939,
  "version": "1.0.0"
}
```

### `GET /api/metrics`

```json
{
  "type": "metrics",
  "ts": 1793158989275,
  "cpu": 35.29,
  "cpu_cores": 4,
  "ram_used": 9899712512,
  "ram_total": 16703565824,
  "ram_pct": 59.27,
  "temp": 70.0,
  "net_rx": 546.2,
  "net_tx": 1061.2,
  "net_rx_total": 707817633,
  "net_tx_total": 384605458,
  "ping": 7.3,
  "uptime": 268939,
  "load": [9.09, 6.94, 5.54],
  "host": "desk",
  "os": "Linux (Ubuntu 24.04)",
  "iface": "eth0"
}
```

`temp` and `ping` may be `null`. A request during the first ~250 ms after startup returns HTTP 503 with `{"ok":false,"error":"warming_up"}`.

### `POST /api/action`

```bash
curl -sS http://127.0.0.1:8787/api/action \
  -H 'content-type: application/json' \
  -H 'x-liquid-token: my-secret' \
  -d '{"action":"media","command":"play_pause"}'
```

Rate limit: 25 actions per second across HTTP and WebSocket. Excess requests receive HTTP 429 or an `action_result` with `ok: false`.

### Deck configuration API

```bash
# read the layout, toggle state and sound library
curl -sS http://127.0.0.1:8787/api/config

# save an edited layout from a file
curl -sS -X PUT http://127.0.0.1:8787/api/config \
  -H 'content-type: application/json' \
  -H 'x-liquid-token: my-secret' \
  --data-binary @boards.json

# reset to the built-in deck
curl -sS -X POST http://127.0.0.1:8787/api/config/reset \
  -H 'x-liquid-token: my-secret'
```

`PUT` returns `200` with the normalised config, the pruned state and the sound list, or `400` with `{"ok":false,"message":"…"}` when validation fails. The server rejects structural problems (unknown action, missing board target, invalid URL) and clamps cosmetic values (labels, colours, columns, spans). A successful save updates the executor’s OBS credentials and broadcasts `{"type":"config",…}` to every client.

Toggle and active-board state:

```bash
curl -sS -X POST http://127.0.0.1:8787/api/state \
  -H 'content-type: application/json' \
  -d '{"board":"gaming","button":"mic","active":true}'

curl -sS -X POST http://127.0.0.1:8787/api/state \
  -H 'content-type: application/json' \
  -d '{"active_board":"media"}'
```

State is keyed per board, so the same button id on two boards keeps independent on/off values. Saving a new layout prunes state for deleted boards and buttons.

### Audio and sounds API

```bash
curl -sS http://127.0.0.1:8787/api/audio

curl -sS -X POST http://127.0.0.1:8787/api/audio \
  -H 'content-type: application/json' \
  -d '{"op":"volume","id":"alsa_output.…","value":45}'

curl -sS http://127.0.0.1:8787/api/sounds
curl -sS 'http://127.0.0.1:8787/api/sound?name=click.wav' --output click.wav
```

`GET /api/audio` returns `{supported, backend, devices, apps, message}`. `devices` contains `{id, description, volume, muted, is_default}`; `apps` contains `{id, app, title, volume, muted}`. When `pactl` is unavailable on Linux, `supported` is `false` and `message` explains what to install.

`POST /api/audio` accepts `op` = `output`, `volume`, `mute`, `app_volume` or `app_mute`, plus `id` and an optional `value` (0–100).

`POST /api/obs/test` accepts `{ "url": "ws://127.0.0.1:4455", "password": "…" }` and returns `{ok, message}` without saving the settings.

### WebSocket protocol

Connect to `/ws` (or `/wss` behind TLS). The server sends `hello` immediately:

```json
{
  "type": "hello",
  "version": "1.0.0",
  "host": "desk",
  "os": "Linux (Ubuntu 24.04)",
  "interval_ms": 1000,
  "history": [{"t": 1793158988000, "rx": 12.4, "tx": 3.1, "cpu": 8.2, "ram": 51.0}],
  "metrics": {"ts": 1793158989000, "cpu": 8.4},
  "actions": ["key", "media", "volume"],
  "auth_required": true,
  "scripts": false,
  "sounds_enabled": true,
  "sounds": [{"name": "click.wav", "size": 3130}],
  "config": {"version": 1, "settings": {}, "boards": []},
  "state": {"toggles": {"gaming/mic": true}, "active_board": "gaming", "updated_at": 1793158989}
}
```

Then:

| Direction | Message |
| --- | --- |
| server → client | `{"type":"metrics","ts":…,"cpu":…,…}` every active sample. |
| server → client | `{"type":"config","config":{…},"state":{…}}` when any client saves or resets the layout. |
| server → client | `{"type":"deck_state","state":{…}}` when a toggle or active board changes. |
| client → server | `{"type":"ping","t":1793158989000}`. |
| server → client | `{"type":"pong","t":1793158989000}`. |
| client → server | `{"type":"action","request_id":"abc","action":"volume","direction":"up","step":5}`. |
| server → client | `{"type":"action_result","request_id":"abc","ok":true,…}`. |
| server → client | `{"type":"error","message":"Invalid JSON payload."}`. |

The server also sends a WebSocket Ping frame every 25 seconds and answers client Ping frames with Pong frames. Browsers do not expose WebSocket Ping frames to JavaScript, which is why the app-level `ping`/`pong` pair exists for RTT display.

### Reverse proxy

The UI uses relative URLs, so it works behind a proxy on any path/host. Preserve the upgrade headers:

```nginx
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_read_timeout 3600s;
}
```

When the page is served over HTTPS, `app.js` automatically uses `wss://`.

---

## Galaxy Note 4 panel over ADB

The Galaxy Note 4 (SM-N910C, 1440 × 2560 AMOLED, Android 10) makes an excellent dedicated panel: the UI is designed for its portrait resolution, the true-black background suits AMOLED, and ADB can wake or sleep it without installing anything on the tablet.

```text
┌────────────────────┐        Wi-Fi / Tailscale         ┌────────────────────┐
│  Galaxy Note 4      │  HTTP :8787 + WebSocket          │  Host (Linux/Win/  │
│  Chrome / PWA       │ ───────────────────────────────► │  macOS)            │
│  dashboard + deck   │ ◄─────────────────────────────── │  liquid-deck       │
└─────────┬──────────┘        ADB :5555                  └─────────┬──────────┘
          │                                                        │
          └──────────────── adb connect / input keyevent ──────────┘
```

### 1. One-time ADB setup

On the Note 4:

1. Open **Settings → About phone** and tap **Build number** seven times.
2. Open **Developer options** and enable **USB debugging**.
3. Connect USB, run `adb devices` on the host, and accept the RSA fingerprint prompt on the tablet.

```bash
adb devices
# List of devices attached
# 0123456789abcdef    device
```

Then switch the daemon to TCP mode (Android 10 does not have the Android 11 “Wireless debugging” pairing UI):

```bash
adb tcpip 5555

# Find the tablet's Wi-Fi address (while still on USB):
adb shell ip addr show wlan0
# or
adb shell ifconfig wlan0

# Unplug USB and connect over Wi-Fi:
adb connect 192.168.1.101:5555
adb devices
```

Give the tablet a DHCP reservation, or use its Tailscale address:

```bash
adb connect 100.91.171.96:5555
```

Notes:

- `adb tcpip 5555` resets after a reboot. For a permanently unwired panel, either keep USB connected, re-run `adb tcpip` after each boot, or use a root/Tasker boot script. A USB connection also charges the tablet.
- ADB port 5555 is powerful. Keep it on a trusted LAN or a Tailscale tailnet with ACLs. Never expose it to the public internet. `adb usb` disables TCP mode.
- If `adb devices` shows `unauthorized`, revoke USB debugging authorizations on the tablet and reconnect. If it shows `offline`, run `adb kill-server`, then `adb connect` again.
- Multiple ADB versions can conflict on port 5037. Use one platform-tools installation, or set `ANDROID_ADB_SERVER_PORT`.

### 2. Screen control commands

| Goal | Command |
| --- | --- |
| Wake | `adb -s 192.168.1.101:5555 shell input keyevent KEYCODE_WAKEUP` |
| Dismiss keyguard | `adb -s 192.168.1.101:5555 shell wm dismiss-keyguard` |
| Sleep | `adb -s 192.168.1.101:5555 shell input keyevent KEYCODE_SLEEP` |
| Screen state | `adb -s 192.168.1.101:5555 shell dumpsys power \| grep -E 'mWakefulness\|Display Power'` |
| Stay on while charging | `adb -s 192.168.1.101:5555 shell svc power stayon true` |
| Normal timeout | `adb -s 192.168.1.101:5555 shell svc power stayon false` |
| Disable timeout entirely | `adb -s 192.168.1.101:5555 shell settings put system screen_off_timeout 2147483647` |
| Restore 30 s timeout | `adb -s 192.168.1.101:5555 shell settings put system screen_off_timeout 30000` |
| Set brightness (0–255) | `adb -s 192.168.1.101:5555 shell settings put system screen_brightness 80` |
| Lock portrait | `adb -s 192.168.1.101:5555 shell settings put system accelerometer_rotation 0` |
| Rotate to portrait | `adb -s 192.168.1.101:5555 shell settings put system user_rotation 0` |
| Immersive fullscreen | `adb -s 192.168.1.101:5555 shell settings put global policy_control immersive.full=*` |
| Restore system bars | `adb -s 192.168.1.101:5555 shell settings put global policy_control null*` |
| Open the dashboard | `adb -s 192.168.1.101:5555 shell am start -a android.intent.action.VIEW -d "http://192.168.1.50:8787/" com.android.chrome` |
| Open without forcing Chrome | `adb -s 192.168.1.101:5555 shell am start -a android.intent.action.VIEW -d "http://192.168.1.50:8787/"` |

`svc power stayon true` only keeps the screen on **while charging**. For a 24/7 wall panel, leave the tablet on USB power, or use the lifecycle/schedule recipes below and let it sleep when unused.

### 3. Helper script

[`integrations/note4-panel.sh`](integrations/note4-panel.sh) wraps the common commands:

```bash
chmod +x integrations/note4-panel.sh

# status is read-only
ADB_TARGET=192.168.1.101:5555 ./integrations/note4-panel.sh status

# wake, dismiss keyguard, open the dashboard
ADB_TARGET=192.168.1.101:5555 \
DECK_URL=http://192.168.1.50:8787/ \
./integrations/note4-panel.sh wake

# sleep
ADB_TARGET=192.168.1.101:5555 ./integrations/note4-panel.sh sleep

# keep the screen on while charging
ADB_TARGET=192.168.1.101:5555 ./integrations/note4-panel.sh stayon
```

`DECK_URL` must be the **host’s** LAN or Tailscale address, not `127.0.0.1` — the URL is opened by the tablet. Start Liquid Deck with `--lan` and a token, then include the token on the first launch:

```text
http://192.168.1.50:8787/?token=my-secret
```

`watch` mode polls `/api/health` and wakes the tablet when `clients` is above zero, then sleeps it after `IDLE_SECS` with no clients:

```bash
ADB_TARGET=192.168.1.101:5555 \
DECK_URL=http://192.168.1.50:8787/ \
IDLE_SECS=120 \
./integrations/note4-panel.sh watch
```

A caveat: a browser can keep its WebSocket open while the tablet screen is off, so `clients` may stay above zero. For unattended panels, the lifecycle hooks and scheduled sleep below are more reliable.

### 4. Recommended: server lifecycle hooks

Wake the panel when the server starts, and sleep it when the server stops.

**Linux systemd**

`/etc/systemd/system/liquid-deck.service`:

```ini
[Unit]
Description=Liquid Deck hardware dashboard
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=deck
WorkingDirectory=/opt/liquid-deck
EnvironmentFile=/etc/liquid-deck.env
Environment=PATH=/usr/local/bin:/usr/bin:/bin
ExecStart=/opt/liquid-deck/liquid-deck --lan
ExecStartPost=/bin/sh -c 'sleep 2; /opt/liquid-deck/integrations/note4-panel.sh wake'
ExecStopPost=/opt/liquid-deck/integrations/note4-panel.sh sleep
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

`/etc/liquid-deck.env`:

```bash
LIQUID_DECK_TOKEN=replace-with-a-long-random-token
LIQUID_DECK_SCRIPTS=/opt/liquid-deck/scripts
ADB_TARGET=192.168.1.101:5555
DECK_URL=http://192.168.1.50:8787/
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now liquid-deck
journalctl -u liquid-deck -f
```

The `sleep 2` in `ExecStartPost` gives the listener time to bind before the tablet opens the URL. If the tablet is slow to reconnect after boot, increase it to 5 seconds.

**macOS launchd**

Use a small wrapper so the panel sleeps when the process exits:

```bash
#!/bin/sh
/opt/liquid-deck/integrations/note4-panel.sh wake
trap '/opt/liquid-deck/integrations/note4-panel.sh sleep' EXIT INT TERM
/opt/liquid-deck/liquid-deck --lan
```

Save as `/opt/liquid-deck/run-deck.sh`, then reference it from a `LaunchAgent` or `LaunchDaemon` with `RunAtLoad` and `KeepAlive`. Put the token in the wrapper’s environment or in a protected `EnvironmentVariables` dictionary in the plist.

**Windows Task Scheduler / PowerShell**

```powershell
$target = "192.168.1.101:5555"
$deck   = "http://192.168.1.50:8787/"

& adb connect $target
& adb -s $target shell input keyevent KEYCODE_WAKEUP
& adb -s $target shell wm dismiss-keyguard
& adb -s $target shell am start -a android.intent.action.VIEW -d $deck com.android.chrome

try {
    $env:LIQUID_DECK_TOKEN = "replace-with-a-long-random-token"
    Start-Process -FilePath "C:\liquid-deck\liquid-deck.exe" `
        -ArgumentList "--lan" -WindowStyle Hidden -Wait
} finally {
    & adb -s $target shell input keyevent KEYCODE_SLEEP
}
```

Create a Task Scheduler task at logon with “Run whether user is logged on or not” and the PowerShell script as the action. Run the task once manually to verify.

### 5. Scheduled wake/sleep

If the panel should follow a daily rhythm rather than the server lifecycle, use cron (Linux/macOS) or Task Scheduler (Windows):

```cron
# Wake at 07:00, sleep at 23:00
0 7  * * * ADB_TARGET=192.168.1.101:5555 DECK_URL=http://192.168.1.50:8787/ /opt/liquid-deck/integrations/note4-panel.sh wake
0 23 * * * ADB_TARGET=192.168.1.101:5555 /opt/liquid-deck/integrations/note4-panel.sh sleep
```

Cron does not inherit your shell environment; set `ADB_BIN`, `ADB_TARGET`, `DECK_URL` and `PATH` inline or in the crontab.

### 6. Kiosk and burn-in tips

- Install the dashboard as a PWA: Chrome menu → **Add to Home screen** / **Install app**. The manifest uses `display: standalone` and a black theme.
- Hide the status and navigation bars with `policy_control immersive.full=*`; restore with `null*`.
- Lock portrait for the 1440 × 2560 layout.
- Lower brightness for a dark room; the UI is already AMOLED black.
- Let the screen sleep when unused. Static bright elements are the main burn-in risk; the ambient orbs are dim and animated slowly.
- `locksettings set-disabled true` can remove the lock screen for a dedicated panel. This weakens physical security; re-enable it with `locksettings set-disabled false`.
- If Chrome feels sluggish on the Note 4’s GPU, enable **Developer options → Remove animations** (the CSS respects `prefers-reduced-motion`) and close background apps. The glass blur is the most expensive effect; the stylesheet also handles `prefers-reduced-transparency` and browsers without `backdrop-filter`.
- Keep the tablet on USB power for 24/7 use. Old Note 4 batteries dislike being held at 100% and heat; check `dumpsys battery` and avoid direct sunlight.

---

## Deployment

### Linux

The systemd unit above is the recommended setup. For a rootless user service, put the unit in `~/.config/systemd/user/` and use `systemctl --user enable --now liquid-deck`; make sure the user’s ADB keys are in `~/.android/adbkey`.

### Windows

- Run the release `.exe` from a terminal, or create a shortcut with `--lan --token ...`.
- To start at logon, use Task Scheduler with a PowerShell wrapper like the one above.
- To run as a service, use [NSSM](https://nssm.cc/) or a Windows service wrapper. Keep the token in the service environment, not in a world-readable script.
- Add a firewall rule for TCP 8787 only on private networks:

```powershell
New-NetFirewallRule -DisplayName "Liquid Deck" -Direction Inbound `
  -Protocol TCP -LocalPort 8787 -Profile Private -Action Allow
```

### macOS

- Run from Terminal, or create a `LaunchAgent` for login startup.
- Grant Accessibility permission to the app that launches `liquid-deck` if you use key/media actions.
- For remote access, bind to the Tailscale interface or use a reverse proxy; do not expose the raw HTTP port.

### Reverse proxy / HTTPS

Liquid Deck speaks plain HTTP. If you need HTTPS, terminate TLS at Caddy, nginx, Traefik or Tailscale Serve/Funnel. Preserve WebSocket upgrade headers and a long read timeout. The frontend automatically switches to `wss://` when the page is HTTPS.

Example Caddyfile:

```caddy
deck.example.ts.net {
    reverse_proxy 127.0.0.1:8787
}
```

---

## Security

Liquid Deck can type keys, change volume, launch programs and run configured scripts. Treat it as a remote-control service.

- **Default to localhost.** The default bind is `127.0.0.1:8787`. Use `--lan` only when you need the tablet to reach it.
- **Use a token on any LAN.** `--token` / `LIQUID_DECK_TOKEN` protects metrics, actions, the deck configuration/state API, audio, sounds, OBS tests and `/ws`. Static UI files and `/api/health` remain public so the lock screen can load.
- **Deck editing is remote code configuration.** Anyone with the token can define URL launchers, scripts, sound names and OBS operations. Treat the token as admin access, not as a viewer password.
- **Sound paths are confined.** Sound names reject separators and `..`, and the canonical path must stay inside the soundboard directory. Only audio extensions are listed or played.
- **OBS credentials stay local.** The password is stored in `boards.json` (mode `0600` on Unix) and sent only to the configured `ws://` endpoint.
- **Tokens are compared in constant time.** They can be supplied via `x-liquid-token`, `Authorization: Bearer`, or `?token=` (needed for browser WebSocket handshakes).
- **The URL token is stored and stripped.** `app.js` writes it to `localStorage` and removes it from the address bar. Avoid pasting tokens into shared screenshots or shell history; rotate if leaked.
- **No TLS.** Use a trusted LAN, Tailscale, or a reverse proxy. A token over plain HTTP can be observed by anyone on the path.
- **Action allow-list.** The network can only request the actions in `ACTION_CATALOG`. It cannot send arbitrary shell commands.
- **Script confinement.** Script names are validated and canonicalized inside `--scripts`. Symlinks that escape the directory are rejected. Scripts still run with the server user’s privileges; use a dedicated low-privilege account.
- **Configuration hooks are trusted.** `LIQUID_DECK_LAUNCH_*` and `LIQUID_DECK_GAME_MODE_*` are administrator-controlled and run through the platform shell. Never derive them from request data.
- **Rate limiting.** 25 actions/second across HTTP and WebSocket; excess requests get 429 or `ok: false`.
- **CSP and headers.** `default-src 'self'`, no framing, `nosniff`, no referrer, restricted permissions policy. The UI uses no external scripts, fonts or images.
- **ADB is powerful.** Port 5555 grants shell access to the tablet. Keep it on a trusted network or Tailscale ACL, and disable TCP mode with `adb usb` when not needed.
- **Run as a non-root user.** Root is not required for metrics or the normal actions. Linux key injection works as the desktop user; macOS needs Accessibility permission; Windows needs an interactive desktop session for `SendKeys`/`keybd_event`.

---

## CI and release artifacts

[`.github/workflows/build.yml`](.github/workflows/build.yml) runs on pushes, pull requests and `v*` tags.

1. **`test`** — `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` on Ubuntu.
2. **`build`** — a matrix of:
   - `x86_64-unknown-linux-musl` → `liquid-deck-linux-x86_64` + `LiquidDeck-x86_64.AppImage`
   - `x86_64-pc-windows-msvc` → `liquid-deck-windows-x86_64.exe`
   - `aarch64-apple-darwin` → `liquid-deck-macos-arm64`
   - `x86_64-apple-darwin` → `liquid-deck-macos-x86_64`
3. **`release`** — on a `v*` tag, downloads all artifacts and publishes a GitHub Release with generated notes.

Artifacts are uploaded per platform and compressed. To cut a release:

```bash
git tag v1.0.0
git push origin v1.0.0
```

The AppImage is built with the official `appimagetool` continuous AppImage and `--appimage-extract-and-run`, so it works on runners without FUSE.

---

## Performance

Measured on the reference Linux x86_64 build in a container, with the release profile:

| Metric | Value |
| --- | --- |
| Release binary | **1.4 MB** |
| RSS, no viewers | **~2.9 MiB** |
| RSS, one live WebSocket viewer | **~3.1 MiB** |
| CPU, idle | effectively **0%** (`ps` reports 0.0) |
| Active sample interval | 1000 ms |
| Idle sample interval | 5000 ms |
| Network history | 60 points |

The browser’s memory is separate and depends on Chrome/WebView. The backend uses a single-threaded Tokio runtime and no TLS stack; that is deliberate. If you need TLS, terminate it at a proxy.

The idle sampler skips `sysinfo` refreshes entirely, and the ping task skips probes when no viewer is active. The first metrics request may return `warming_up` for about 250 ms while CPU counters settle.

---

## Troubleshooting

### The server starts but the dashboard is blank

- Check `http://127.0.0.1:8787/api/health` with `curl`.
- Open the browser devtools console. A CSP error means a proxy is injecting scripts; keep the page same-origin.
- If the page loads but metrics stay at `--`, the WebSocket may be blocked. Check the connection pill: `Live`, `Polling` or `Offline`. A proxy must forward `Upgrade` and `Connection`.

### `cannot bind 0.0.0.0:8787`

Another process owns the port. Find it with `ss -ltnp | grep 8787` (Linux), `lsof -i :8787` (macOS), or `Get-NetTCPConnection -LocalPort 8787` (Windows), or start with `--port 9000`.

### The tablet cannot reach the host

- Start with `--lan`, not the default localhost bind.
- Allow TCP 8787 through the host firewall.
- Use the host’s LAN IP (`ip addr`, `ipconfig`, `ifconfig`), not `127.0.0.1`.
- Check that both devices are on the same VLAN and that client isolation is off.
- For Tailscale, use the host’s `100.x.y.z` address and allow the port in the tailnet ACL.

### `401 Unauthorized` / the lock screen keeps appearing

- The server has a token; enter it in the lock screen or visit `/?token=...` once.
- Clear the stored token: devtools → Application → Local Storage → delete `liquid-deck.token`.
- If you rotated `LIQUID_DECK_TOKEN`, restart the server and clear the old browser token.
- `curl` needs `-H 'x-liquid-token: ...'` or `?token=...`.

### `temperature` is `null`

`sysinfo` reads available `hwmon` sensors. VMs, containers and some laptops expose none. Load the right kernel module (`coretemp`, `k10temp`, `it87`, etc.) or accept `null`; the UI shows `—`.

### Media or volume actions fail on Linux

- Install `playerctl` for media and `pactl`/`wpctl`/`amixer` for volume.
- On Wayland, install `wtype`; on X11, install `xdotool`.
- Flatpak/Snap sandboxing can hide audio sockets. Run Liquid Deck outside the sandbox or grant access.
- Check the action response `message`; it includes the failing backend’s stderr.

### Key actions fail on macOS

Grant Accessibility permission to the terminal or binary. `osascript` returns a permission error otherwise. Also check **Privacy & Security → Automation** if controlling Music/Spotify.

### Key actions fail on Windows

`WScript.Shell.SendKeys` and `keybd_event` need an interactive desktop session. Running as a Windows service in session 0 will not type into the user’s apps. Use Task Scheduler at logon instead.

### Scripts do not run

- Start with `--scripts /absolute/path` (or `LIQUID_DECK_SCRIPTS`).
- The script must be inside that directory after canonicalization.
- `.sh` files are run with `sh` if not executable; other files need the executable bit.
- Check the response `message`; stderr is included and truncated.
- The timeout is 10 seconds.

### Soundboard buttons do nothing

- Run the server with `--sounds <DIR>` (or `LIQUID_DECK_SOUNDS`). `--no-sounds` disables the soundboard entirely, and the editor then shows “Soundboard disabled”.
- Only files with a supported audio extension are listed. Check the **Integrations → Soundboard** list; if a clip is missing, its extension is probably unsupported.
- Linux needs one player on `PATH`: `paplay`, `pw-play`, `ffplay`, `mpv` or `aplay`. Install `pulseaudio-utils` / `pipewire-audio` for the first two.
- The startup banner prints the resolved **Sounds** directory. Copy clips there; the server does not rescan the filesystem on every keystroke, so refresh the page after adding files.
- If the clip previews in the browser but the hardware plays nothing, the server is reaching a different audio session. Check `XDG_RUNTIME_DIR` / `PULSE_SERVER` when running as a service.

### OBS actions fail or the test times out

- OBS Studio 28+ is required, with **Tools → WebSocket Server Settings → Enable WebSocket server** turned on. The default port is `4455`.
- The URL must start with `ws://` — for example `ws://127.0.0.1:4455`. `wss://` is rejected unless you put a TLS proxy in front of OBS and point the URL at the proxy.
- Copy the password from OBS into **Integrations**. An empty password is valid when OBS authentication is disabled.
- `connection refused` means OBS is closed or the port is wrong. A timeout usually means a firewall or a wrong host when OBS runs on another machine.
- OBS only accepts one authenticated session per action; a very fast burst of OBS buttons may briefly queue. The action error includes the obs-websocket reason (`AuthenticationFailed`, `RequestStatus`…).
- Scene and source names are case-sensitive and must match exactly, including spaces and emoji.

### The audio mixer says “Mixer unavailable”

- Linux requires `pactl` (`pulseaudio-utils`, also shipped by PipeWire’s PulseAudio shim). Verify with `pactl info`.
- If `pactl` exists but lists no devices, the server user cannot reach the session bus. Export `XDG_RUNTIME_DIR=/run/user/$(id -u)` in the service unit.
- macOS needs `SwitchAudioSource` (`brew install switchaudio-osx`) for output switching. Without it, the mixer can still change volume through AppleScript but cannot switch outputs.
- Windows reports the mixer as unsupported; use the volume buttons and media keys instead.
- Application rows only appear while something is actually playing audio (`pactl list sink-inputs` is empty otherwise).
- Slider changes are sent on release, not while dragging. If nothing changes, check the toast for the `pactl` error text.

### ADB says `unauthorized`, `offline` or `no route to host`

- Reconnect USB, accept the RSA prompt, then `adb tcpip 5555` again.
- `adb kill-server && adb start-server && adb connect <ip>:5555`.
- Confirm the tablet’s IP has not changed; use a DHCP reservation.
- `adb tcpip` resets on reboot. Re-run it over USB.
- On Tailscale, verify `tailscale status` and that port 5555 is allowed by the ACL.
- If `adb connect` hangs, another ADB server may own port 5037; stop it or set `ANDROID_ADB_SERVER_PORT`.

### The Note 4 wakes but Chrome shows a connection error

The host URL is wrong or the server is not listening yet. Use the host LAN/Tailscale IP, start with `--lan`, and add a delay before `note4-panel.sh wake` in the service unit. Check the token if the lock screen appears instead.

### The AppImage does not start

Install FUSE, or run:

```bash
./LiquidDeck-x86_64.AppImage --appimage-extract-and-run
```

### The UI is slow on the Note 4

The blur is GPU-heavy. Enable **Developer options → Remove animations**, close background apps, and avoid extremely high brightness. The CSS falls back to opaque panels when `backdrop-filter` is unavailable and honors `prefers-reduced-transparency` / `prefers-reduced-motion`.

### WebSocket works locally but not through a proxy

Enable HTTP/1.1 and the upgrade headers, and raise the read timeout. Some CDNs buffer WebSocket traffic; use a proxy/route that supports upgrades.

---

## Project layout

```text
liquid-deck/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs          # server, config, auth, routes, WebSocket
│   ├── metrics.rs       # sysinfo sampler, history, TCP ping
│   ├── deck.rs          # boards.json model, validation, state persistence
│   ├── actions.rs       # action validation + platform backends
│   ├── audio.rs         # output/application mixer
│   ├── obs.rs           # obs-websocket v5 client
│   └── sounds.rs        # soundboard library and playback
├── web/
│   ├── index.html
│   ├── style.css
│   ├── app.js
│   ├── icon.svg
│   ├── icon-192.png
│   ├── icon-512.png
│   ├── noise.png
│   └── manifest.webmanifest
├── assets/
│   ├── icon.png
│   └── sounds/          # embedded seed clips (click/chime/alert)
├── integrations/
│   └── note4-panel.sh   # optional ADB wake/sleep helper
├── .github/workflows/
│   └── build.yml
├── .gitignore
├── LICENSE
└── README.md
```

---

## License

MIT. See [LICENSE](LICENSE).
