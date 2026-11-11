# Packaging

Build scripts for the three desktop deliverables. Every script writes into
`dist/` at the repository root and uses the version from `Cargo.toml`.

| Script | Output | Needs |
| --- | --- | --- |
| `build-windows.sh` | `liquid-deck-<ver>-windows-x86_64.exe` | `cargo-xwin`, rustup target `x86_64-pc-windows-msvc`, `clang`/`lld` |
| `build-arch.sh` | `liquid-deck-<ver>-1-x86_64.pkg.tar.zst` | `makepkg` (Arch, Manjaro, CachyOS…), `cargo`, network for crates |
| `build-appimage.sh` | `liquid-deck-<ver>-x86_64.AppImage` | `cargo`, network (downloads `appimagetool` once) |

```bash
packaging/build-windows.sh
packaging/build-arch.sh
packaging/build-appimage.sh
```

## Windows

Cross-compiled from Linux to the MSVC target through `cargo-xwin`, which
downloads the MSVC headers, libraries and CRT on first use and drives `clang`
and `lld`. No mingw toolchain is involved.

The CRT is linked statically (`-C target-feature=+crt-static`), so the exe is a
single file that runs without the Visual C++ redistributable. It is unsigned:
SmartScreen shows "More info → Run anyway" on first launch. The app lives in the
tray; `--console` runs it with a console window instead, and `--help` prints the
flags.

A native Windows build works too:

```powershell
cargo build --release
```

## Arch package

`packaging/arch/PKGBUILD` builds from a tarball of the current working tree, so a
local build packages exactly the code in the checkout. It installs the binary,
the hicolor icons, the desktop entry and the AppStream metainfo file, and pulls
no runtime dependencies beyond `glibc`/`gcc-libs` (`playerctl` and
`nvidia-utils` are optional, for media metadata and NVIDIA telemetry).

`build-arch.sh` stages the tarball, runs `makepkg -f` and copies the resulting
package to `dist/`. When `cargo` comes from rustup instead of the pacman package,
the script adds `--nodeps` so the build is not rejected over a missing
`makedepends` entry.

Inspect the result with:

```bash
pacman -Qip dist/liquid-deck-1.0.0-1-x86_64.pkg.tar.zst
pacman -Qlp dist/liquid-deck-1.0.0-1-x86_64.pkg.tar.zst
sudo pacman -U dist/liquid-deck-1.0.0-1-x86_64.pkg.tar.zst
```

For a published package, point `source=` at the GitHub release tarball and
replace `sha256sums=('SKIP')` with the real checksum.

## AppImage

`build-appimage.sh` builds the release binary, assembles an `AppDir` (binary,
icon, `.desktop`, metainfo, `AppRun`) and packages it with `appimagetool`, which
is downloaded once into `~/.cache/liquid-deck-packaging`. The desktop entry and
the metainfo file share the `io.github.serkanagdas.LiquidDeck` id, so AppStream
validation in `appimagetool` passes.

Run it with:

```bash
./dist/liquid-deck-1.0.0-x86_64.AppImage            # normal start
./dist/liquid-deck-1.0.0-x86_64.AppImage --help     # server flags
./dist/liquid-deck-1.0.0-x86_64.AppImage --appimage-extract-and-run   # no FUSE
```

## Started at login (Linux)

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/liquid-deck.service <<'EOF'
[Unit]
Description=Liquid Deck dashboard
[Service]
ExecStart=/usr/bin/liquid-deck
Restart=on-failure
[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now liquid-deck
```

On Windows the tray menu has a "start at logon" toggle, or use
`liquid-deck --enable-startup` / `--disable-startup`.

## CI

`.github/workflows/build.yml` builds the Linux (musl), Windows (MSVC) and macOS
binaries and an AppImage on every tag. Add `packaging/arch/PKGBUILD` to an Arch
container job if you want the `.pkg.tar.zst` published as well.
