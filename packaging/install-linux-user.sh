#!/usr/bin/env bash
# Install the release binary and a desktop launcher without sudo.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$HOME/.local/lib/liquid-deck"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
mkdir -p "$BIN" "$HOME/.local/bin" "$APPS" "$ICONS"
# Atomic replacement also works when an older copy is running.
install -m755 "$ROOT/target/release/liquid-deck" "$BIN/liquid-deck.new"
mv -f "$BIN/liquid-deck.new" "$BIN/liquid-deck"
install -m644 "$ROOT/web/icon-512.png" "$ICONS/liquid-deck.png"
cat > "$HOME/.local/bin/liquid-deck" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
BINARY="$HOME/.local/lib/liquid-deck/liquid-deck"
# Explicit arguments retain CLI behavior (including --no-tray and custom ports).
if (( $# )); then exec "$BINARY" "$@"; fi
exec 9>"${XDG_RUNTIME_DIR:-$HOME/.cache}/liquid-deck-launch.lock"
if ! flock -n 9; then
    exec 9>&-
    exec xdg-open http://127.0.0.1:8787/
fi
if curl -fsS --max-time 1 http://127.0.0.1:8787/api/health >/dev/null 2>&1; then
    exec 9>&-
    exec xdg-open http://127.0.0.1:8787/
fi
(
    exec 9>&-
    for ((i=0; i<30; i++)); do
        if curl -fsS --max-time 1 http://127.0.0.1:8787/api/health >/dev/null 2>&1; then
            exec xdg-open http://127.0.0.1:8787/
        fi
        sleep 0.2
    done
) >/dev/null 2>&1 &
exec "$BINARY"
LAUNCHER
chmod 755 "$HOME/.local/bin/liquid-deck"
cat > "$APPS/liquid-deck.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Liquid Deck
Comment=Donanım panosu ve Stream Deck
Exec="$HOME/.local/bin/liquid-deck"
Icon=$ICONS/liquid-deck.png
Terminal=false
Categories=Utility;
StartupNotify=false
DESKTOP
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$APPS"
fi
echo "Installed: $HOME/.local/bin/liquid-deck"
echo "Open Liquid Deck from your application menu."
