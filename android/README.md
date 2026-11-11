# Robin — native Android client

This folder contains a native **Kotlin + Jetpack Compose** client for the Liquid
Deck PC agent. It is a thin client: the Rust binary on the PC still samples the
hardware, controls media and runs the deck actions. The APK replaces the
browser, not the server.

## What works

- **Sistem** — a hero row mirroring the web dashboard: two identical cards
  (CPU and GPU) with the same anatomy — label, thread count / model, then the
  temperature as plain text on the left and two minimal speedometer rings on the
  right: **Kullanım** on top, **Fan** directly under it. A bottom line carries
  the sensor source (CPU) or VRAM plus clocks and power (GPU). Above the row,
  one slim strip with ↓ İndirme, ↑ Yükleme, TCP Ping and RAM. Tap either card to
  open its detail sheet, which holds the per-thread grid, motherboard fan
  headers, CPU power and the full GPU clock/power/p-state readout.
  The rings are 270° arcs with a number and a small `%` in the middle; a missing
  sensor draws an empty track with `—` instead of inventing a reading. The same
  scale colours the arc, the number and the detail sheet values:
  **0–40 % white · 40–70 % orange · 70–100 % red**. RAM uses the same scale.
  CPU/GPU temperatures use their own scale on both cards and detail panels:
  **≤ 60 °C white · > 60–80 °C orange · > 80 °C red**. CPU card/detail accents
  are crimson red; status and quota success colours remain unchanged.
- **Medya** — now-playing metadata, album art, a smooth progress bar and
  play/pause, previous/next, volume and mute through the PC's media session.
- **Ses** — every render endpoint the PC reports on one page: Bluetooth
  headphones, USB headset, analog output, HDMI monitor. The current default is
  the green card at the top of the list; tap another card and the PC switches to
  it immediately, moving the streams that are already playing too. Each card
  shows its kind (`BLUETOOTH`, `HDMI`, `ANALOG`, `KULAKLIK`, `USB`) and the
  device volume when the backend can report it. The list polls every 4 seconds
  while the page is visible, so a headset connected mid-session appears on its
  own.
- **Deck** — every board and button from `boards.json`, including nested
  boards, per-board columns, button spans, colors, icons, subtitles and
  persistent toggle state. Tap the pencil in the tab row to edit keys on the
  phone: change the label, subtitle, color, icon, action and width, save a new
  key, or delete one. The whole config goes back to the server with
  `PUT /api/config`, so the web editor and the phone always agree.
- **Kota** — the last page tracks how much is left of your AI subscriptions.
  Providers are configured on the phone and queried straight from it, so the PC
  can stay off; the list refreshes every **3 minutes** (Claude at most every
  5 minutes, because its endpoint throttles). A card is mostly dial: the
  provider name and your display name share one line, then up to two equal
  analog-digital rings, each with a short window caption underneath
  (`5 SAAT`, `7 GÜN`, `DÖNEM`, `LİMİT`) and its reset countdown. The ring,
  number and percentage follow the remaining quota — green ≥ 50 %,
  orange 20–50 %, red below 20 %. Providers that only report money show the
  balance instead. Tap a card to edit it, use **+ Sağlayıcı ekle** for a new one.
- **Bağlantı** — WebSocket with exponential-backoff reconnect, HTTP polling
  fallback, token auth, manual host entry, saved settings.
- **Panel** — keep-screen-on, haptic feedback, AMOLED black, CarPlay-style
  status strip and dock. Immersive fullscreen: Android's status and navigation bars are hidden while
  the app or its floating panels are open. Swipe from an edge to reveal the
  system controls temporarily; no phone setting, root or kiosk lock is needed.
  The header shows the date beside the time, without a
  connected badge; offline warnings remain visible. The five pages sit in a horizontal pager: swipe left
  or right anywhere on the page area to move to the next/previous page, and the
  dock highlight follows the swipe; tapping a dock item animates to that page.
- **Yatay öncelikli** — the activity runs in `sensorLandscape`, so a tablet in a
  landscape mount always fills the screen. The status strip and dock get thinner
  on short panels (≤ ~520 dp), the hero row stays two equal columns, and the
  now-playing strip and GPU clocks only appear when there is height for them.
  A portrait stack remains as a fallback for unusual devices.

Not in this version: board renaming/columns, the per-app audio mixer, soundboard,
OBS test UI and history charts. Those remain in the web UI; the app says so in
Settings. Output switching is on the **Ses** page.

## Requirements

- Android **6.0 (API 23)** or newer — this includes the Galaxy Note 4.
- A PC running the `liquid-deck` server on the same network.
- For building: JDK 17+ (JDK 21 recommended) and Android SDK 35.

Verified on an Android 15 (API 35) emulator and on a Galaxy Note 4
(`SM-N910C`, Android 10) in landscape: connect flow, live metrics, the
CPU/GPU hero row with its gauges and detail sheets, media transport, board
switching, toggles, the in-app key editor, settings sheet, the Kota grid and
its provider editor (manual card, live Z.ai request, 401 error card), offline
banner and automatic reconnect. The Ses page was checked on the same Note 4:
all four outputs listed with their kinds and volumes, tapping Arctis Nova 7
switched the PC default, the toast reported `Output switched (11 streams moved)`,
and the green card moved to the new device within one poll.

## Build

```bash
cd android

# Point Gradle at your SDK if ANDROID_HOME is not set
echo "sdk.dir=$HOME/Android/Sdk" > local.properties

./gradlew :app:assembleDebug
```

The APK is written to:

```text
android/app/build/outputs/apk/debug/app-debug.apk
```

Install it over USB:

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Or copy the APK to the phone and open it (allow installation from unknown
sources once).

For a smaller, non-debuggable build, open the project in Android Studio and use
**Build → Generate Signed App Bundle / APK**, or add your own `signingConfig`.
`assembleRelease` produces an unsigned APK, so it needs a keystore first.

## Run

1. Start the server on the PC, reachable from the LAN:

   ```bash
   cargo run --release -- --lan --token "gizli-sifreniz"
   ```

2. Open the app. Enter the PC address and port, for example
   `192.168.1.50:8787`, plus the token if you set one.
3. The app verifies `/api/health` and `/api/auth`, then opens
   `ws://<host>/ws?token=...`.

The server prints the LAN address and port on startup. Allow port `8787` through
the PC firewall.

If the phone still cannot reach the port although both devices are on the same
Wi-Fi (some routers and firewalls block client-to-client traffic), tunnel it
over the adb connection instead:

```bash
adb reverse tcp:8787 tcp:8787
```

Then enter `127.0.0.1:8787` in the app. The tunnel goes away when the cable is
unplugged or adb disconnects, so it is a testing workaround rather than a
replacement for a reachable LAN port.

The connection test above only covers the PC; the Kota page needs working
internet from the tablet itself. If the phone reports "Ağa ulaşılamadı" while
other IP-based apps still work, check **Ayarlar → Ağ ve internet → Özel DNS**.
A stale encrypted-DNS hostname takes every name lookup down with it — AdGuard's
old `dns.adguard.com` is retired and must be `dns.adguard-dns.com`, or set
Private DNS to **Otomatik** / **Kapalı**. Verify from the PC:

```bash
adb shell ping -c 1 google.com   # "unknown host" = DNS, not Wi-Fi
```

On Linux the now-playing panel comes from `playerctl`
(`sudo pacman -S playerctl` / `sudo apt install playerctl`). Without it the
Media page keeps saying "nothing playing"; metrics, audio and deck actions work
regardless.

GPU telemetry on Linux comes from `nvidia-smi` when the NVIDIA driver is
installed, or straight from the `amdgpu` sysfs files otherwise. Motherboard fan
headers are read from `/sys/class/hwmon`, so the kernel needs a SuperIO driver
for your board: `sudo modprobe nct6775` (Nuvoton) or `sudo modprobe it87`
(ITE), added to `/etc/modules-load.d/` to survive reboots. Without one the fan
card stays hidden and the CPU card shows only the CPU temperature.

## Ses — output routing

`GET /api/audio` returns the render endpoints plus the apps that are currently
playing; the app uses the first half. `POST /api/audio` with
`{"op":"output","id":"…"}` switches the default. The same operation backs the
audio keys in the web deck, so both stay in sync.

| Platform | Backend | What a switch does |
| --- | --- | --- |
| **Linux** | `pactl` (PipeWire or PulseAudio) | `set-default-sink`, then `move-sink-input` for every open stream, so audio is audible immediately; the reply reports how many streams moved |
| **Windows** | Core Audio COM | enumerates `IMMDevice` render endpoints, reads `PKEY_Device_FriendlyName`, and calls `IPolicyConfig::SetDefaultEndpoint` for the console, multimedia and communications roles |
| **macOS** | `SwitchAudioSource` | changes the default output when the tool is installed |

Volume on the cards comes from the sink itself, which only the `pactl` backend
reports; on Windows the cards show the kind alone and `POST /api/audio` with
`op=volume` answers that the operation is unavailable.

## Kota — subscription quota

Keys never leave the phone: they are stored in a separate DataStore
(`liquid_deck_quota`) and are only sent to the vendor's own API. Everything is
read-only; the app never spends credits or changes a plan.

| Provider | Endpoint | What you get | Key you need |
| --- | --- | --- | --- |
| **Z.ai** | `/api/monitor/usage/quota/limit` | remaining % of the 5-hour and weekly token windows, reset times, plan | personal **Coding Plan** key — a standard API key reports no quota |
| **Claude** | `/api/oauth/usage` | remaining % of the 5-hour and 7-day windows, reset times | `accessToken` from `~/.claude/.credentials.json` after a Claude Code login (expires; paste a fresh one when the card turns red) |
| **OpenRouter** | `/api/v1/credits` + `/api/v1/key` | key limit percentage, or remaining credits | `sk-or-…` |
| **DeepSeek** | `/user/balance` | balance (¥/$); a monthly budget turns it into a percentage | `sk-…` |
| **OpenAI** | `/v1/organization/costs` | this month's spend; a monthly budget turns it into a percentage | an **admin** key with `org:read` |
| **Manuel** | — | a percentage you set with the slider | none |

- **ChatGPT Plus/Pro subscription quota has no public API.** OpenAI only exposes
  organisation *costs* to admin keys, so a ChatGPT plan is tracked as a Manuel
  card (or as spend with a budget) rather than as remaining messages.
- The same holds for Gemini, Copilot, Cursor and similar plans: add them as
  **Manuel** and update the slider when you check the vendor's page.
- Optional **monthly budget** (OpenRouter, DeepSeek, OpenAI) converts a balance
  or a spend into a percentage; leave it empty to see the raw money figure.
- A failed refresh keeps the last good numbers and adds the error line, so a
  flaky connection does not blank a card.

### Adding another provider

`QuotaKind` in `data/QuotaModels.kt` lists the sources, and each one has a small
adapter in `data/QuotaRepository.kt` (`zai`, `claude`, `openRouter`, `deepSeek`,
`openAi`, `manual`). Add an enum entry, a `fetch` branch that returns a
`QuotaSnapshot`, and the card, editor and refresh loop pick it up automatically.
Anything without an endpoint still works through `manual`.

## How it is put together

```text
android/app/src/main/java/com/liquiddeck/app/
├── MainActivity.kt          edge-to-edge shell, keep-screen-on, snackbar
├── data/
│   ├── Models.kt            wire models (metrics, boards, state, events)
│   ├── AudioModels.kt       audio devices, kind heuristic, snapshot
│   ├── DeckRepository.kt    OkHttp WebSocket, polling fallback, action RPC
│   ├── SettingsStore.kt     DataStore: host, token, keep-awake, haptics
│   ├── QuotaModels.kt       provider kinds, quota windows, snapshots
│   ├── QuotaRepository.kt   one adapter per provider, ISO date parsing
│   └── QuotaStore.kt        DataStore: quota providers and their keys
└── ui/
    ├── DeckViewModel.kt     connection + deck state, action payloads
    ├── ConnectScreen.kt     first-run host/token screen
    ├── MainShell.kt         status strip, pages, dock
    ├── SystemPage.kt        live hardware cards, gauges, detail sheet
    ├── MediaPage.kt         artwork, progress, transport controls
    ├── AudioPage.kt         output cards, switch on tap, 4 s poll while visible
    ├── DeckPage.kt          board tabs and the touch grid
    ├── QuotaPage.kt         quota grid, provider cards, editor sheet
    ├── QuotaViewModel.kt    three-minute refresh loop, provider CRUD
    ├── KeyEditorSheet.kt    on-phone key editor (label, color, icon, action)
    ├── SettingsSheet.kt     connection settings
    ├── DeckSheet.kt         shared dark floating panel, fixed close header
    ├── Components.kt        glass cards, gauge ring, status pill
    ├── Format.kt            byte/rate/uptime formatting, colors, icons
    └── theme/Theme.kt       AMOLED palette and typography
```

### Protocol

The client speaks the same protocol as `web/app.js`:

| Direction | Message | Purpose |
| --- | --- | --- |
| server → app | `hello` | version, metrics, deck config, toggle state |
| server → app | `metrics` | one sample, every `LIQUID_DECK_INTERVAL_MS` |
| server → app | `config` | board layout changed (from the web editor) |
| server → app | `deck_state` | toggle or active-board change |
| app → server | `action` + `request_id` | run a deck action |
| server → app | `action_result` | matched by `request_id` |
| app → server | `ping` / `pong` | round-trip measurement |

If the socket is down, actions fall back to `POST /api/action` and telemetry to
`GET /api/metrics` every 2 seconds.

### Notes

- Cleartext HTTP is enabled on purpose: the agent has no TLS stack and is
  meant for a trusted LAN. Do not expose port 8787 to the internet; use a
  reverse proxy or a VPN such as Tailscale if you need remote access.
- The token is stored in the app's private DataStore. It is not encrypted at
  rest, same as the browser's `localStorage`.
- Back gestures close the settings sheet. The deck keeps the active board in
  sync with the server through `POST /api/state`.
- Cover art and seeking come from the Windows GSMTC and macOS AppleScript
  backends. The Linux `playerctl` backend reports title, artist, album, state
  and position, but no artwork and no seek support, so the Media page shows the
  artwork placeholder there.
- Settings, CPU/GPU details, key editing and provider editing share `DeckSheet`:
  a centred dark panel capped at 700 dp, a fixed close header, subtle accent
  gradient, rounded border and bounded scrollable content. Keyboard insets are
  handled by the shared panel; no blur or continuous decorative motion is used.
- The Kota API keys are stored in plain DataStore, like the server token. Keep
  the tablet locked, and revoke a key from the vendor's dashboard if it leaks.
