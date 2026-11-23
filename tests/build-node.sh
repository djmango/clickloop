#!/usr/bin/env bash
# Build clickloop on this host for Linux and Windows.
#
# rdev on Linux links against X11 (XTest, XRecord), so the build needs the X11
# development headers and pkg-config. They come from nix-shell here rather than
# from a system package manager.
set -euo pipefail
export PATH=/etc/profiles/per-user/skg/bin:$PATH

cd /tmp
rm -rf clickloop-build rdev-local
mkdir -p clickloop-build rdev-local
tar xzf /tmp/clickloop-clean.tgz -C /tmp
mv /tmp/clickloop-src/* /tmp/clickloop-build/ 2>/dev/null || true
tar xf /tmp/rdev-clean.tar -C /tmp/rdev-local

cd /tmp/clickloop-build
mkdir -p .cargo
cat > .cargo/config.toml <<'CFG'
[patch."https://github.com/djmango/rdev"]
rdev = { path = "/tmp/rdev-local" }
CFG

echo "=== linux build ==="
nix-shell -p pkg-config xorgproto libX11 libXtst libXi libxcb --run \
  "cd /tmp/clickloop-build && cargo build --release 2>&1 | tail -20" || echo "LINUX BUILD FAILED"

if [ -x target/release/clickloop ]; then
  echo "--- linux binary ---"
  ls -la target/release/clickloop
  file target/release/clickloop
  ./target/release/clickloop --list-keys | head -2
fi

echo "=== windows build ==="
nix-shell -p pkg-config xorgproto libX11 libXtst libXi libxcb --run \
  "cd /tmp/clickloop-build && cargo build --release --target x86_64-pc-windows-gnu 2>&1 | tail -25" \
  || echo "WINDOWS BUILD FAILED"
ls -la target/x86_64-pc-windows-gnu/release/clickloop.exe 2>/dev/null || echo "no exe"
