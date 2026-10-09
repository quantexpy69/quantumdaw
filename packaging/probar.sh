#!/usr/bin/env bash
# Prueba los paquetes de dist/ en contenedores limpios de cada familia de distribuciones:
# instala el paquete y comprueba que el programa arranca (quantum-daw --version) y sus archivos.
# Uso: packaging/probar.sh   (necesita podman o docker y haber ejecutado packaging/build.sh)
set -uo pipefail
cd "$(dirname "$0")/.."
ENGINE=$(command -v podman || command -v docker)
ok=0; mal=0
probar() { # imagen, comando
  if "$ENGINE" run --rm -v "$PWD/dist":/dist:ro,Z "$1" bash -c "$2" >/tmp/qdaw-prueba.log 2>&1; then
    echo "✔ $1 — $(grep -o 'Quantum DAW [0-9.]*' /tmp/qdaw-prueba.log | head -1)"; ok=$((ok + 1))
  else
    echo "✘ $1"; tail -5 /tmp/qdaw-prueba.log; mal=$((mal + 1))
  fi
}
CHECK='quantum-daw --version && test -f /usr/share/applications/quantum-daw.desktop && test -f /usr/share/icons/hicolor/scalable/apps/quantum-daw.svg'
probar docker.io/library/debian:12 "apt-get -qq update && apt-get -qq install -y /dist/quantum-daw-amd64.deb >/dev/null && $CHECK"
probar docker.io/library/ubuntu:20.04 "apt-get -qq update && DEBIAN_FRONTEND=noninteractive apt-get -qq install -y /dist/quantum-daw-amd64.deb >/dev/null && $CHECK"
probar docker.io/library/ubuntu:24.04 "apt-get -qq update && DEBIAN_FRONTEND=noninteractive apt-get -qq install -y /dist/quantum-daw-amd64.deb >/dev/null && $CHECK"
probar docker.io/rockylinux/rockylinux:8 "dnf -y -q --setopt=install_weak_deps=False install /dist/quantum-daw-x86_64.rpm && $CHECK"
probar docker.io/library/almalinux:9 "dnf -y -q --setopt=install_weak_deps=False install /dist/quantum-daw-x86_64.rpm && $CHECK"
probar quay.io/centos/centos:stream9 "dnf -y -q --setopt=install_weak_deps=False install /dist/quantum-daw-x86_64.rpm && $CHECK"
probar docker.io/library/fedora:latest "dnf -y -q --setopt=install_weak_deps=False install /dist/quantum-daw-x86_64.rpm && $CHECK"
probar docker.io/library/archlinux:latest "pacman -Sy --noconfirm --needed alsa-lib >/dev/null && cd /tmp && tar xzf /dist/quantum-daw-linux-x86_64.tar.gz && cd quantum-daw-* && ./instalar.sh --sistema >/dev/null && quantum-daw --version && grep -q '^pkgname=quantum-daw-bin' /dist/PKGBUILD"
probar docker.io/library/ubuntu:22.04 "apt-get -qq update && apt-get -qq install -y libasound2 >/dev/null && cd /tmp && cp /dist/quantum-daw-x86_64.AppImage . && ./quantum-daw-x86_64.AppImage --appimage-extract >/dev/null && squashfs-root/AppRun --version"
echo "Resultado: $ok correctas, $mal con errores"
[ "$mal" -eq 0 ]
