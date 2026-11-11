#!/usr/bin/env bash
# Builds a portable AppImage from the release binary.
# Output: dist/liquid-deck-<version>-x86_64.AppImage
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# cargo lives in ~/.cargo/bin on most Rust installs but is not always on PATH.
[ -d "$HOME/.cargo/bin" ] && case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) PATH="$HOME/.cargo/bin:$PATH" ;; esac
VER="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1 | tr -d '\r')"
NAME="liquid-deck-$VER-x86_64.AppImage"
APPDIR="$ROOT/AppDir"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/liquid-deck-packaging"
TOOL="$CACHE/appimagetool-x86_64.AppImage"

cd "$ROOT"
echo "==> cargo build --release"
cargo build --release --locked

echo "==> AppDir hazırlanıyor"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/icons/hicolor/512x512/apps"

install -m755 target/release/liquid-deck "$APPDIR/usr/bin/liquid-deck"
install -m644 web/icon-512.png "$APPDIR/liquid-deck.png"
install -m644 web/icon-512.png "$APPDIR/.DirIcon"
install -m644 web/icon-512.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/liquid-deck.png"
install -m644 packaging/arch/io.github.serkanagdas.LiquidDeck.desktop \
  "$APPDIR/io.github.serkanagdas.LiquidDeck.desktop"
install -m644 packaging/arch/io.github.serkanagdas.LiquidDeck.desktop \
  "$APPDIR/usr/share/applications/io.github.serkanagdas.LiquidDeck.desktop"
# The metainfo file name has to match the desktop entry id, otherwise AppStream
# validation fails with metainfo-filename-cid-mismatch.
install -m644 packaging/arch/io.github.serkanagdas.LiquidDeck.appdata.xml \
  "$APPDIR/usr/share/metainfo/io.github.serkanagdas.LiquidDeck.appdata.xml"
install -m644 LICENSE "$APPDIR/LICENSE"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# AppImage entry point: hand the arguments straight to the server binary.
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/liquid-deck" "$@"
EOF
chmod +x "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
  echo "==> appimagetool indiriliyor"
  mkdir -p "$CACHE"
  curl -fsSL -o "$TOOL" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$TOOL"
fi

echo "==> AppImage paketleniyor"
mkdir -p dist
# Absolute paths: appimagetool hands the output path to mksquashfs, which does
# not reliably honour a relative destination path.
ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" "$ROOT/dist/$NAME"

echo "==> dist/$NAME"
ls -lh "dist/$NAME"
