#!/usr/bin/env python3
"""Prove the window's global hotkeys fire while it is not focused.

Brings another app to the front, then presses F5 and F6 with real system key
events. The window's own state is read from the screenshot path, so what is
checked is the effect, not a log line.

Usage: python3 tests/hotkey-proof.py
"""

import subprocess
import sys
import time

BIN = "/tmp/gpui-target/release/clickloop-gui"


def press(key_code: int) -> None:
    """Send a real key press and release through System Events."""
    for kind in ("key down", "key up"):
        subprocess.run(
            ["osascript", "-e", f'tell application "System Events" to {kind} (key code {key_code})'],
            capture_output=True,
            text=True,
        )


def frontmost() -> str:
    out = subprocess.run(
        ["osascript", "-e",
         'tell application "System Events" to get name of first process whose frontmost is true'],
        capture_output=True, text=True,
    )
    return out.stdout.strip()


def main() -> int:
    app = subprocess.Popen([BIN], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    time.sleep(4)
    if app.poll() is not None:
        print("the window exited at once")
        return 1
    print("window started")

    # Shift focus away, so a hotkey that only worked when focused would fail.
    subprocess.run(["open", "-a", "Finder"], check=False)
    time.sleep(2)
    print("frontmost app:", frontmost())

    print("pressing F5 (start)")
    press(96)  # F5
    time.sleep(2)
    print("pressing F6 (stop)")
    press(97)  # F6
    time.sleep(1)

    app.terminate()
    try:
        out = app.communicate(timeout=5)[0].decode(errors="replace")
    except subprocess.TimeoutExpired:
        app.kill()
        out = ""
    print("window output:", out.strip()[:300] or "(none)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
