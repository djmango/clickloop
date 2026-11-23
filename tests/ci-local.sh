#!/usr/bin/env bash
# Run the CI steps locally, so a billing-blocked runner is not the only check.
#
# Each step matches the workflow: build, test, smoke test, list keys, format,
# clippy. Run from the repo root on any of the three systems.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== build (release) =="
cargo build --release

echo "== test =="
cargo test

echo "== smoke test (dry run, as CI runs it) =="
cargo run --release -- --actions "(1,1,click)*2" --dry-run --start-delay 0 --count 1

echo "== every key name parses =="
cargo run --release -- --list-keys > /dev/null
echo "list-keys ok"

echo "== format =="
cargo fmt --check
echo "fmt ok"

echo "== clippy =="
cargo clippy --all-targets -- -D warnings
echo "clippy ok"

echo "== differential test against the Swift original =="
if [ -f "$HOME/Downloads/auto-click.swift" ]; then
  python3 tests/differential.py
else
  echo "skipped: no Swift original at \$HOME/Downloads/auto-click.swift"
fi

echo
echo "all CI steps passed"
