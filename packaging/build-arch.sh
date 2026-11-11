#!/usr/bin/env bash
# Builds a pacman-installable package from the current working tree.
# Output: dist/liquid-deck-<version>-1-x86_64.pkg.tar.zst
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# cargo lives in ~/.cargo/bin on most Rust installs but is not always on PATH,
# and makepkg resolves makedepends through PATH.
[ -d "$HOME/.cargo/bin" ] && case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) PATH="$HOME/.cargo/bin:$PATH" ;; esac
VER="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1 | tr -d '\r')"
PKG="liquid-deck"
ARCH_DIR="$ROOT/packaging/arch"
TARBALL="$ARCH_DIR/$PKG-$VER.tar.gz"
OUT="$ROOT/dist"

if ! command -v makepkg >/dev/null; then
  echo "makepkg bulunamadı: Arch/Manjaro/CachyOS dışında paket üretilemez." >&2
  exit 1
fi

echo "==> Kaynak tarball hazırlanıyor ($PKG-$VER)"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/$PKG-$VER"

tar -C "$ROOT" \
  --exclude=./target --exclude=./dist --exclude=./AppDir \
  --exclude=./android --exclude=./.git --exclude=./.openchamber \
  --exclude=./packaging/arch/src --exclude=./packaging/arch/pkg \
  --exclude='*.tar.gz' --exclude='*.AppImage' --exclude='*.exe' \
  --exclude='*.pkg.tar.zst' -cf - . | tar -C "$STAGE/$PKG-$VER" -xf -

tar -C "$STAGE" -czf "$TARBALL" "$PKG-$VER"

echo "==> makepkg"
cd "$ARCH_DIR"
rm -rf src pkg
# rustup's cargo satisfies the build but is not a pacman package, so the
# dependency check is skipped when `cargo` comes from ~/.cargo/bin.
NODEPS=()
if ! pacman -T cargo >/dev/null 2>&1; then
  echo "    cargo pacman paketi yok; rustup cargo kullanılıyor (--nodeps)"
  NODEPS+=(--nodeps)
fi
makepkg -f --noconfirm "${NODEPS[@]}"

mkdir -p "$OUT"
cp -f "$ARCH_DIR/$PKG-$VER"-*.pkg.tar.zst "$OUT/"
echo "==> $OUT/$PKG-$VER-1-x86_64.pkg.tar.zst"
pacman -Qp "$OUT/$PKG-$VER"-*.pkg.tar.zst 2>/dev/null || true

# makepkg unpacks the sources into packaging/arch/src and stages the package
# under packaging/arch/pkg; both are build output and must stay out of git.
rm -rf "$ARCH_DIR/src" "$ARCH_DIR/pkg" "$TARBALL"
rm -f "$ARCH_DIR/$PKG-$VER"-*.pkg.tar.zst
