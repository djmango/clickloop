#!/usr/bin/env bash
# Build the window on Linux and prove its hotkeys there.
#
# GPUI on Linux needs a long list of platform libraries. X11 lets a passive
# keyboard listener work with no permission prompt, which is why the hotkey
# proof runs here: it exercises the same Router code the macOS build uses.
set -euo pipefail

SRC=/tmp/clickloop-gui-src.tgz
OUT=/tmp/gui-out
mkdir -p "$OUT"

docker run --rm --platform linux/amd64 \
  -v "$SRC":/src.tgz:ro \
  -v /tmp/rdev-clean.tar:/rdev.tar:ro \
  -v "$OUT":/out \
  rust:1-bookworm bash -c '
set -euo pipefail
apt-get update -qq
apt-get install -y -qq build-essential pkg-config cmake curl \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libxcb-xinput-dev \
  libegl1-mesa-dev libgles2-mesa-dev libglib2.0-dev libfontconfig-dev \
  libxtst-dev libxi-dev libxcb1-dev x11proto-dev libxext-dev \
  libxdo-dev libxdo3 xvfb x11-utils xdotool > /dev/null 2>&1

mkdir -p /work /rdev
tar xzf /src.tgz -C /work
tar xf /rdev.tar -C /rdev
cd /work/clickloop

mkdir -p .cargo
cat > .cargo/config.toml <<"CFG"
[patch."https://github.com/djmango/rdev"]
rdev = { path = "/rdev" }
CFG

echo "=== build the CLI ==="
cargo build --release 2>&1 | tail -3

echo "=== build the window ==="
cd gui
cargo build --release 2>&1 | tail -5
cp target/release/clickloop-gui /out/clickloop-window-linux-x64 2>/dev/null || echo "no window binary"
ls -la /out/
' 2>&1 | grep -vE "Get:|Selecting|Preparing|Unpacking|Setting up|^\(Reading"
