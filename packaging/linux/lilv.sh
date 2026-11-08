#!/usr/bin/env bash
# Compila lilv (plugins LV2) y sus dependencias como librerías estáticas en el prefijo indicado,
# para enlazarlas dentro del binario: así los paquetes funcionan aunque el sistema no tenga lilv
# (AlmaLinux 8, donde se compilan, no lo trae; tampoco EPEL). Necesita meson, ninja y un compilador de C.
# Uso: packaging/linux/lilv.sh <prefijo>     (no hace nada si ya está compilado)
set -euo pipefail
PREFIJO=${1:?Uso: lilv.sh <prefijo>}
[ -f "$PREFIJO/lib/liblilv-0.a" ] && exit 0
FUENTES=$(mktemp -d)
trap 'rm -rf "$FUENTES"' EXIT
export PKG_CONFIG_PATH="$PREFIJO/lib/pkgconfig:$PREFIJO/share/pkgconfig:${PKG_CONFIG_PATH:-}"
# Mismas versiones que Ubuntu 24.04 (la API que usan livi y lilv-sys).
compilar() {
  local url=$1 dir=$2
  shift 2
  curl -sSfL "$url" | tar -xJ -C "$FUENTES"
  meson setup "$FUENTES/$dir/build" "$FUENTES/$dir" --prefix="$PREFIJO" --libdir=lib --buildtype=release \
    --default-library=static -Dtests=disabled -Ddocs=disabled "$@" >/dev/null
  meson install -C "$FUENTES/$dir/build" >/dev/null
}
compilar https://lv2plug.in/spec/lv2-1.18.10.tar.xz lv2-1.18.10 -Dplugins=disabled
compilar https://download.drobilla.net/zix-0.4.2.tar.xz zix-0.4.2 -Dbenchmarks=disabled -Dtests_cpp=disabled
compilar https://download.drobilla.net/serd-0.32.2.tar.xz serd-0.32.2 -Dtools=disabled -Dman=disabled
compilar https://download.drobilla.net/sord-0.16.16.tar.xz sord-0.16.16 -Dtools=disabled
compilar https://download.drobilla.net/sratom-0.6.16.tar.xz sratom-0.6.16
compilar https://download.drobilla.net/lilv-0.24.22.tar.xz lilv-0.24.22 -Dtools=disabled -Dbindings_py=disabled -Dbindings_cpp=disabled
echo "lilv estático en $PREFIJO"
