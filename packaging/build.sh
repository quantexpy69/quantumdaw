#!/usr/bin/env bash
# Compila Quantum DAW contra glibc 2.28 (AlmaLinux 8) y crea en dist/ los paquetes de Linux:
#   quantum-daw-linux-x86_64.tar.gz  portable (cualquier distribución, con instalar.sh)
#   quantum-daw-amd64.deb            Debian, Ubuntu, Linux Mint, Pop!_OS…
#   quantum-daw-x86_64.rpm           Fedora, RHEL, CentOS Stream, Rocky Linux, AlmaLinux, openSUSE
#   quantum-daw-x86_64.AppImage      universal, sin instalar
#   PKGBUILD                         Arch Linux, Manjaro, EndeavourOS (makepkg -si)
#   SHA256SUMS                       sumas de verificación
# Uso en tu equipo (con podman o docker): packaging/build.sh
# Dentro del contenedor o en GitHub Actions: packaging/build.sh --dentro
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/app/Cargo.toml | head -1)

if [[ "${1:-}" != "--dentro" ]]; then
  ENGINE=$(command -v podman || command -v docker) || { echo "Necesitas podman o docker."; exit 1; }
  mkdir -p .toolchain/contenedor
  exec "$ENGINE" run --rm -v "$PWD":/src:Z -v "$PWD/.toolchain/contenedor":/cache:Z -w /src \
    docker.io/library/almalinux:8 bash packaging/build.sh --dentro
fi

# ---------- Dentro de AlmaLinux 8 (glibc 2.28) ----------
CACHE=${CACHE:-/cache}
dnf -y -q install gcc gcc-c++ make diffutils alsa-lib-devel pkgconf-pkg-config curl git file xz findutils >/dev/null
export CARGO_HOME="$CACHE/cargo" RUSTUP_HOME="$CACHE/rustup" CARGO_TARGET_DIR="$CACHE/target"
export PATH="$CARGO_HOME/bin:$PATH" PKG_CONFIG_PATH=/usr/lib64/pkgconfig
command -v cargo >/dev/null || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
command -v cargo-deb >/dev/null || cargo install -q cargo-deb --locked
command -v cargo-generate-rpm >/dev/null || cargo install -q cargo-generate-rpm --locked
git config --global --add safe.directory "$PWD" 2>/dev/null || true

cargo build --release --locked -p quantum-daw
BIN="$CARGO_TARGET_DIR/release/quantum-daw"
"$BIN" --version
rm -rf dist && mkdir -p dist

# Portable .tar.gz con su instalador.
PKG="dist/quantum-daw-$VERSION"
mkdir -p "$PKG"
cp "$BIN" packaging/linux/instalar.sh packaging/linux/quantum-daw.desktop packaging/linux/com.quantumdaw.QuantumDAW.metainfo.xml LICENSE README.md "$PKG/"
cp assets/quantumlogo.svg "$PKG/quantum-daw.svg"
tar -C dist -czf dist/quantum-daw-linux-x86_64.tar.gz "quantum-daw-$VERSION"
rm -rf "$PKG"

# .deb y .rpm.
cargo deb -p quantum-daw --no-build --no-strip -o dist/quantum-daw-amd64.deb
cargo generate-rpm -p crates/app --target-dir "$CARGO_TARGET_DIR" -o dist/quantum-daw-x86_64.rpm

# AppImage (universal).
APPDIR="$CACHE/AppDir"
rm -rf "$APPDIR" && mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/scalable/apps" "$APPDIR/usr/share/metainfo"
cp "$BIN" "$APPDIR/usr/bin/"
cp packaging/linux/quantum-daw.desktop "$APPDIR/" && cp packaging/linux/quantum-daw.desktop "$APPDIR/usr/share/applications/"
cp assets/quantumlogo.svg "$APPDIR/quantum-daw.svg" && cp assets/quantumlogo.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/quantum-daw.svg"
cp packaging/linux/com.quantumdaw.QuantumDAW.metainfo.xml "$APPDIR/usr/share/metainfo/"
ln -sf usr/bin/quantum-daw "$APPDIR/AppRun"
TOOL="$CACHE/appimagetool-x86_64.AppImage"
[ -x "$TOOL" ] || { curl -sSfL -o "$TOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage && chmod +x "$TOOL"; }
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$TOOL" -n "$APPDIR" dist/quantum-daw-x86_64.AppImage >/dev/null

# PKGBUILD de Arch que instala el .tar.gz de esta versión.
SUM=$(sha256sum dist/quantum-daw-linux-x86_64.tar.gz | cut -d' ' -f1)
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA256@/$SUM/" packaging/arch/PKGBUILD.in > dist/PKGBUILD

(cd dist && sha256sum quantum-daw-* PKGBUILD > SHA256SUMS)
ls -lh dist
