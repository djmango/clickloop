#!/usr/bin/env bash
# Build the Linux x86-64 binaries in a Debian container, so the result links
# against a system glibc rather than the nix store.
#
# Run on the hermes node: the container is linux/amd64 either way.
set -euo pipefail

OUT=/tmp/musl-out
mkdir -p "$OUT"

docker run --rm --platform linux/amd64 \
  -v /tmp/clickloop-clean.tgz:/src.tgz:ro \
  -v /tmp/rdev-clean.tar:/rdev.tar:ro \
  -v "$OUT":/out \
  rust:1-bookworm bash -c '
set -euo pipefail
apt-get update -qq
apt-get install -y -qq libx11-dev libxtst-dev libxi-dev libxcb1-dev libxproto-dev pkg-config

mkdir -p /work /rdev
tar xzf /src.tgz -C /work 2>/dev/null
tar xf /rdev.tar -C /rdev
cd /work/clickloop-src

# Point the rdev git dependency at the local checkout.
cat > .cargo/config.toml <<"CFG"
[patch."https://github.com/djmango/rdev"]
rdev = { path = "/rdev" }
CFG

echo "=== glibc build (Debian 12, runs on most distros) ==="
cargo build --release --target x86_64-unknown-linux-gnu 2>&1 | tail -4
cp target/x86_64-unknown-linux-gnu/release/clickloop /out/clickloop-linux-x64
strip /out/clickloop-linux-x64 || true

echo "=== musl build (static, runs anywhere) ==="
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1
apt-get install -y -qq musl-tools >/dev/null
cargo build --release --target x86_64-unknown-linux-musl 2>&1 | tail -4 \
  && cp target/x86_64-unknown-linux-musl/release/clickloop /out/clickloop-linux-x64-static \
  && strip /out/clickloop-linux-x64-static \
  || echo "musl build skipped"

ls -la /out/
/out/clickloop-linux-x64 --list-keys | head -2
' 2>&1 | grep -vE "LIBARCHIVE|^debconf|Get:|Selecting|Preparing|Unpacking|Setting up"
