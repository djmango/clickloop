#!/usr/bin/env bash
# Prove the stop key works on Linux.
#
# The tool is started with a long time limit and a stop key of escape, then
# escape is pressed from another process. If the run ends early with "stop key"
# as its reason, the listener path works: XRecord received the key, the chord
# matched, and the run loop noticed.
set -uo pipefail
export PATH=/etc/profiles/per-user/skg/bin:$PATH

BIN="${1:-/tmp/clickloop-build/target/release/clickloop}"
LOG=/tmp/stopproof.log
: > "$LOG"

nix-shell -p xorgserver xdotool --run '
set -uo pipefail
pkill -f "Xvfb :78" 2>/dev/null || true
sleep 1
Xvfb :78 -screen 0 800x600x24 >/tmp/xvfb78.log 2>&1 &
XPID=$!
sleep 3
export DISPLAY=:78

echo "== run with a 30s limit, stop key escape =="
'"$BIN"' --actions "wait:0.05" --start-delay 0 --duration 30 \
  --confirm-keys none --stop-key escape > /tmp/stop-run.log 2>&1 &
RUN=$!
sleep 3

echo "run is alive: $(kill -0 $RUN 2>/dev/null && echo yes || echo no)"
echo "pressing escape"
xdotool key --clearmodifiers Escape
sleep 2

if kill -0 $RUN 2>/dev/null; then
  echo "STILL RUNNING after escape (stop key failed)"
  kill $RUN 2>/dev/null || true
else
  wait $RUN 2>/dev/null
  echo "run ended, exit: $?"
fi

echo "== the tool said =="
cat /tmp/stop-run.log

echo "== also test: a plain second run with no stop key, killed by count =="
'"$BIN"' --actions "a" --start-delay 0 --duration 30 --count 2 \
  --confirm-keys none --stop-key none > /tmp/count-run.log 2>&1
echo "exit: $?"
tail -1 /tmp/count-run.log

kill $XPID 2>/dev/null || true
' >> "$LOG" 2>&1

grep -vE "LIBARCHIVE|evaluation warning|copying path|these .* paths|Nix search|^warning:" "$LOG"
