#!/usr/bin/env bash
# Cross-builds the Windows release binary from Linux with cargo-xwin
# (MSVC toolchain through clang/lld; no mingw needed).
# Output: dist/liquid-deck-<version>-windows-x86_64.exe
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -d "$HOME/.cargo/bin" ] && case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) PATH="$HOME/.cargo/bin:$PATH" ;; esac
VER="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1 | tr -d '\r')"
TARGET="x86_64-pc-windows-msvc"
NAME="liquid-deck-$VER-windows-x86_64.exe"

if ! command -v cargo-xwin >/dev/null; then
  echo "cargo-xwin bulunamadı, kuruluyor…"
  cargo install cargo-xwin --locked
fi

if ! rustup target list --installed | grep -qx "$TARGET"; then
  echo "==> rustup target add $TARGET"
  rustup target add "$TARGET"
fi

cd "$ROOT"
echo "==> cargo xwin build --release --target $TARGET"
# Static CRT: the exe runs on a clean Windows without the VC++ redistributable.
RUSTFLAGS="-C target-feature=+crt-static" \
  cargo xwin build --release --target "$TARGET"

mkdir -p dist
install -m755 "target/$TARGET/release/liquid-deck.exe" "dist/$NAME"
echo "==> dist/$NAME"
ls -lh "dist/$NAME"
