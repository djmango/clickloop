#!/usr/bin/env bash
# Prove the Windows build works, by running the real .exe under Wine.
#
# Wine is not a Windows kernel, but it is a genuine PE loader with real
# USER32: SendInput and the low-level keyboard hook both run. This checks the
# parts that would break on Windows alone: argument parsing, the key table,
# the sequence engine and the dry-run/error paths.
set -uo pipefail
export PATH=/etc/profiles/per-user/skg/bin:$PATH

EXE=/tmp/clickloop-build/target/x86_64-pc-windows-gnu/release/clickloop.exe
LOG=/tmp/wineproof.log
: > "$LOG"

nix-shell -p wineWow64Packages.stable --run '
set -uo pipefail
export WINEPREFIX=$HOME/wineprefix
export WINEDEBUG=-all
EXE=/tmp/clickloop-build/target/x86_64-pc-windows-gnu/release/clickloop.exe

echo "== identity =="
wine "$EXE" --help > /tmp/w-help.txt 2>&1; echo "help exit: $?"
head -5 /tmp/w-help.txt

echo "== key table =="
wine "$EXE" --list-keys > /tmp/w-keys.txt 2>&1; echo "list-keys exit: $?"
head -2 /tmp/w-keys.txt
grep -c "" /tmp/w-keys.txt

echo "== sequence engine (dry run) =="
wine "$EXE" --actions "((a,b)*2,space)*3" --dry-run --start-delay 0 --count 1 > /tmp/w-seq.txt 2>&1
echo "dry-run exit: $?"
cat /tmp/w-seq.txt

echo "== chords and keypad names =="
wine "$EXE" --actions "ctrl+shift+s,kp_plus,f24,wait:0.1" --dry-run --start-delay 0 --count 1 > /tmp/w-chord.txt 2>&1
echo "chord exit: $?"
grep -E "Sequence|dry-run" /tmp/w-chord.txt

echo "== an error must match the Swift wording =="
wine "$EXE" --actions "notakey" --dry-run --start-delay 0 --count 1 > /tmp/w-err.txt 2>&1
echo "error exit: $?"
cat /tmp/w-err.txt

echo "== bad option =="
wine "$EXE" --nope > /tmp/w-opt.txt 2>&1
echo "bad option exit: $?"
cat /tmp/w-opt.txt

echo "== real input: SendInput through Wine =="
wine "$EXE" --actions "a" --start-delay 0 --duration 3 --count 1 \
  --confirm-keys none --stop-key none > /tmp/w-real.txt 2>&1
echo "real-run exit: $?"
cat /tmp/w-real.txt
' >> "$LOG" 2>&1

grep -vE "LIBARCHIVE|evaluation warning|copying path|these .* paths|Nix search|^warning:" "$LOG"
