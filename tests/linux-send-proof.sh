#!/usr/bin/env bash
# Prove the Linux build sends real input.
#
# Xvfb gives a headless X server and xev gives a window that prints every key it
# receives. The tool presses keys into that window, and xev's log is the
# evidence: if the keys appear there, the send path works on Linux.
set -uo pipefail
export PATH=/etc/profiles/per-user/skg/bin:$PATH

BIN="${1:-/tmp/clickloop-build/target/release/clickloop}"
LOG=/tmp/sendproof.log
: > "$LOG"

nix-shell -p xorgserver xev xdpyinfo xdotool --run '
set -uo pipefail
echo "== xvfb =="
pkill -f "Xvfb :77" 2>/dev/null || true
sleep 1
Xvfb :77 -screen 0 800x600x24 >/tmp/xvfb.log 2>&1 &
XPID=$!
sleep 3
export DISPLAY=:77
xdpyinfo | head -3 || echo "no display"

echo "== xev =="
xev -geometry 400x300+0+0 -event keyboard > /tmp/xev.log 2>&1 &
XEV=$!
sleep 3

WID=$(xdotool search --name "Event Tester" 2>/dev/null | head -1 || true)
echo "xev window id: ${WID:-none}"
if [ -n "${WID:-}" ]; then
  xdotool windowfocus "$WID" 2>/dev/null || true
  xdotool windowactivate "$WID" 2>/dev/null || true
fi
sleep 1

echo "== press a,b,c with clickloop =="
'"$BIN"' --actions "a,b,c" --start-delay 0 --duration 3 --count 1 \
  --confirm-keys none --stop-key none > /tmp/clickloop-run.log 2>&1
echo "clickloop exit: $?"
tail -4 /tmp/clickloop-run.log

sleep 2
echo "== xev received =="
grep -cE "^KeyPress" /tmp/xev.log 2>/dev/null || echo "0 (xev log missing)"
grep -E "^KeyPress" /tmp/xev.log 2>/dev/null | head -10

kill $XEV 2>/dev/null || true
kill $XPID 2>/dev/null || true
' >> "$LOG" 2>&1

grep -vE "LIBARCHIVE|evaluation warning|copying path|these .* paths|Nix search path" "$LOG"
