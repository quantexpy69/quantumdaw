#!/usr/bin/env bash
# Compila e instala Quantum DAW desde el código para el usuario actual (sin sudo), en cualquier
# distribución: binario, icono y lanzador en el menú de aplicaciones.
set -euo pipefail
cd "$(dirname "$0")"
source ./env.sh

# Rust: el del proyecto (.toolchain); si no está, se instala ahí mismo con rustup.
if ! command -v cargo >/dev/null; then
  echo "Instalando Rust en .toolchain/ (solo para este proyecto)…"
  curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
fi

# Cabeceras de ALSA (audio) y de lilv (plugins LV2), necesarias para compilar.
if ! pkg-config --exists lilv-0 2>/dev/null || { ! pkg-config --exists alsa 2>/dev/null && [ ! -f .toolchain/sysroot/usr/lib64/pkgconfig/alsa.pc ]; }; then
  . /etc/os-release 2>/dev/null || true
  echo "Faltan las herramientas de compilación o las cabeceras de ALSA o lilv. Instálalas con:"
  case "${ID:-} ${ID_LIKE:-}" in
    *debian* | *ubuntu*) echo "  sudo apt install build-essential pkg-config libasound2-dev liblilv-dev" ;;
    *fedora* | *rhel* | *centos* | *rocky* | *alma*) echo "  sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel lilv-devel" ;;
    *arch* | *manjaro* | *endeavouros*) echo "  sudo pacman -S --needed base-devel alsa-lib lilv" ;;
    *suse*) echo "  sudo zypper install gcc pkg-config alsa-devel lilv-devel" ;;
    *) echo "  el compilador de C, pkg-config y los paquetes de desarrollo de ALSA y lilv de tu distribución" ;;
  esac
  exit 1
fi

# El configure de LAME (mp3lame-sys) rechaza rutas con espacios: si la carpeta del proyecto los
# tiene, el código de los crates y la compilación van a ~/.cache/quantum-daw.
TARGET=target
if [[ "$PWD" == *" "* ]]; then
  CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/quantum-daw"
  export CARGO_HOME="$CACHE/cargo" CARGO_TARGET_DIR="$CACHE/target"
  TARGET="$CARGO_TARGET_DIR"
  echo "La ruta del proyecto tiene espacios: se compila en $CACHE"
fi

# Con la carpeta pro/ (repositorio privado) se incluyen los plugins QUANTUM Pro.
PRO=()
[ -f pro/mod.rs ] && PRO=(--features pro)
cargo build --release ${PRO[@]+"${PRO[@]}"}
BIN="$HOME/.local/bin/quantum-daw"
install -Dm755 "$TARGET/release/quantum-daw" "$BIN"
# Icono: SVG y PNG de cada tamaño (algunos paneles y docks prefieren PNG).
ICONOS="$HOME/.local/share/icons/hicolor"
install -Dm644 assets/quantumlogo.svg "$ICONOS/scalable/apps/quantum-daw.svg"
for t in 16:icon_16x16 32:icon_32x32 64:icon_32x32@2x 128:icon_128x128 256:icon_256x256 512:icon_512x512; do
  install -Dm644 "assets/macos/QuantumDAW.iconset/${t#*:}.png" "$ICONOS/${t%%:*}x${t%%:*}/apps/quantum-daw.png"
done
mkdir -p "$HOME/.local/share/applications"
sed "s|@BIN@|$BIN|" assets/quantum-daw.desktop > "$HOME/.local/share/applications/quantum-daw.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
# El escritorio guarda los iconos en caché: se le avisa que cambiaron (sin index.theme hace falta -t).
touch "$ICONOS"
gtk-update-icon-cache -q -f -t "$ICONOS" 2>/dev/null || true
echo "Listo: busca «QUANTUM» en el menú de aplicaciones. Vuelve a ejecutar ./install.sh tras cada cambio."
