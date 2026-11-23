#!/usr/bin/env bash
# Prove chords and clicks on Linux.
#
# xev shows the modifier state on each key, and a click shows up as button
# events. The tool is asked for cmd+s (meta), shift+space and a click, and xev's
# log is the evidence.
set -uo pipefail
export PATH=/etc/profiles/per-user/skg/bin:$PATH

BIN="${1:-/tmp/clickloop-build/target/release/clickloop}"
LOG=/tmp/sendproof2.log
: > "$LOG"

nix-shell -p xorgserver xev xdpyinfo xdotool --run '
set -uo pipefail
pkill -f "Xvfb :77" 2>/dev/null || true
sleep 1
Xvfb :77 -screen 0 800x600x24 >/tmp/xvfb.log 2>&1 &
XPID=$!
sleep 3
export DISPLAY=:77

xev -geometry 400x300+0+0 > /tmp/xev2.log 2>&1 &
XEV=$!
sleep 3
WID=$(xdotool search --name "Event Tester" 2>/dev/null | head -1 || true)
if [ -n "${WID:-}" ]; then
  xdotool windowfocus "$WID" 2>/dev/null || true
  xdotool windowactivate "$WID" 2>/dev/null || true
fi
sleep 1

echo "== chord: ctrl+shift+s =="
'"$BIN"' --actions "ctrl+shift+s" --start-delay 0 --duration 3 --count 1 \
  --confirm-keys none --stop-key none > /tmp/run-chord.log 2>&1
echo "exit: $?"
sleep 1
echo "-- key events and their state --"
grep -E "^KeyPress|^KeyRelease|state 0x" /tmp/xev2.log | head -24

echo "== click =="
: > /tmp/xev2.log
'"$BIN"' --actions "click,right-click" --start-delay 0 --duration 3 --count 1 \
  --confirm-keys none --stop-key none > /tmp/run-click.log 2>&1
echo "exit: $?"
sleep 1
echo "-- button events --"
grep -E "^ButtonPress|^ButtonRelease" /tmp/xev2.log | head -10

kill $XEV 2>/dev/null || true
kill $XPID 2>/dev/null || true
' >> "$LOG" 2>&1

grep -vE "LIBARCHIVE|evaluation warning|copying path|these .* paths|Nix search" "$LOG"
