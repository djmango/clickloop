#!/usr/bin/env bash
# Prove the window's global hotkeys fire while it is NOT focused.
#
# Runs inside Debian, where the GUI's shared libraries are a normal apt install
# rather than a pile of nix store paths. Xvfb provides a display with no
# permission gate, so a passive listener works the way it does on a real Linux
# desktop.
#
# The evidence is the window's own history file: it is only written when a run
# starts, and a run starts only from the hotkey or the button.
set -uo pipefail

apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq --no-install-recommends \
  libxcb1 libxkbcommon0 libxkbcommon-x11-0 libx11-6 libxtst6 libfontconfig1 \
  libgl1 libegl1 libglib2.0-0 xvfb x11-utils xdotool xterm fontconfig \
  >/dev/null 2>&1

BIN=/out/clickloop-window-linux-x64
LOG=/out/hotkey-proof.log
: > "$LOG"

echo "unresolved libs: $(ldd "$BIN" 2>/dev/null | grep -c 'not found')" >> "$LOG"

Xvfb :85 -screen 0 1100x800x24 >/tmp/xvfb85.log 2>&1 &
XPID=$!
sleep 3
export DISPLAY=:85

export HOME=/tmp/hotkey-home
rm -rf "$HOME"
mkdir -p "$HOME"

echo "== start the window ==" >> "$LOG"
"$BIN" >> "$LOG" 2>&1 &
APP=$!
sleep 8
if ! kill -0 "$APP" 2>/dev/null; then
  echo "window exited at once" >> "$LOG"
  kill $XPID 2>/dev/null
  cat "$LOG"
  exit 1
fi
echo "window pid $APP" >> "$LOG"

# Focus another window, so a focused-only hotkey would fail here.
xterm -geometry 40x10+600+400 >/dev/null 2>&1 &
XT=$!
sleep 3
echo "active window: $(xdotool getactivewindow 2>/dev/null || echo none)" >> "$LOG"
echo "history before: [$(cat "$HOME/.config/clickloop/history.txt" 2>/dev/null | tr '\n' ' ')]" >> "$LOG"

echo "== press F5 (start) ==" >> "$LOG"
xdotool key F5
sleep 4
echo "history after F5: [$(cat "$HOME/.config/clickloop/history.txt" 2>/dev/null | tr '\n' ' ')]" >> "$LOG"

echo "== press F6 (stop) ==" >> "$LOG"
xdotool key F6
sleep 2
echo "still running after F6: $(kill -0 "$APP" 2>/dev/null && echo yes || echo no)" >> "$LOG"

kill "$APP" 2>/dev/null
kill "$XT" 2>/dev/null
kill "$XPID" 2>/dev/null
cat "$LOG"
