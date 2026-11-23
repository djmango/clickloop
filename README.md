# clickloop

Repeat a sequence of key presses and mouse clicks. One binary, one command, on
macOS, Windows and Linux.

A Rust port of a macOS `auto-click.swift` script. The sequence language and the
safety behaviour are the original's; the platform layer is a single crate
([`rdev`](https://github.com/djmango/rdev)) that maps one key name onto each
system's own key code.

```console
$ clickloop
Sequence: press 1 x2, left-click. Runs for 5.0 seconds. Stop key: escape.
Starting in 3.0 seconds. Focus the target window.
...
Stopped: time limit reached. Cycles: 1284. Elapsed: 5.01 seconds.
```

## Install

```console
$ cargo install --path .
```

Or build a release binary:

```console
$ cargo build --release      # target/release/clickloop
```

## Use

```
clickloop [options]

Safety options:
  --duration SECONDS   Stop after this time. Default: 5, or unlimited with --hotkey.
                       Use 0 for no time limit.
  --stop-key KEY       Key that stops the script. Default: escape. Use "none" to disable.
  --count N            Stop after N cycles. Default: unlimited.
  --start-delay SEC    Seconds before the first cycle. Default: 3.
  --confirm-keys LIST  Keys that need a yes before the run starts, for keys that are
                       risky to press by accident. Default: "8,9,0".

Sequence options:
  --actions LIST       Comma separated sequence. Default: "1,1,click".
  --delay SECONDS      Seconds between cycles. Default: 0.
  --hotkey KEY         Run the sequence once per key press instead of repeating.
  --list-keys          Print every key name and stop.

Other:
  --dry-run            Print the actions, send no input.
  -h, --help           Print this text.
```

### The sequence language

An action is a key name, `click`, `right-click`, or `wait:0.5`. Actions are
comma separated.

```
clickloop --actions "space*3,click"          # repeat one item
clickloop --actions "(1,1,click)*5"          # repeat a group as a unit
clickloop --actions "((a,b)*2,space)*4"      # groups nest and multiply
clickloop --actions "cmd+s"                   # a chord
clickloop --actions "wait:0.5,space,wait:0.5"
```

A group multiplies as a unit, so `(1,1,click)*3` is those three actions three
times, and `((a,b)*2)*3` is `a,b` twelve times. The largest expansion is
1,000,000 actions, so a runaway nest cannot exhaust memory.

Modifier spellings are the same on every OS: `cmd`, `meta`, `super` and `win`
all mean the Command key on macOS and the Windows/Super key elsewhere, so one
sequence runs unchanged everywhere.

### Stopping

A run stops on whichever comes first: the time limit, the cycle count, the stop
key, Control-C, or Enter in the terminal. The stop key is listened for
globally, so it works while another window has focus, and it passes through to
that window rather than being swallowed.

`--hotkey` changes the mode: instead of repeating, the sequence runs once per
key press, and the key also reaches the focused app.

## Platform notes

**macOS** needs two grants for the terminal that runs it, under System Settings
> Privacy & Security: **Accessibility** (to send input) and **Input Monitoring**
(to hear the stop key). Without them macOS silently delivers nothing, so the
tool checks Accessibility up front and says so.

**Windows** needs no permission. Input is sent with `SendInput`; the stop key
uses a low-level keyboard hook.

**Linux** uses X11 only: XTest to send and XRecord to hear the stop key. Wayland
is not supported, because neither has a portable Wayland equivalent. X11 is what
`rdev` targets on this platform.

## What changed from the Swift original

The sequence grammar, the defaults, the safety options and the diagnostic
messages are unchanged, and a 51-case differential test compares the two
programs' output case by case.

The port adds:

- One key table for all three systems, so `cmd+s` or `super+s` works everywhere.
- More key names: `f21` to `f24`, `insert`, `printscreen`, `numlock`, the media
  keys, `kp_*` short forms for keypad keys, and `shiftleft`/`ctrlright` style
  spellings next to the original `leftshift`/`rightctrl`.
- `middle-click` is accepted as an alias for a left click, because the sequence
  language had no middle button.
- Chords are built from explicit modifier key presses rather than a flags field,
  which is what Windows and Linux need and what macOS needs when the event tap
  has not seen the modifier yet.
- A settle time of a few milliseconds after each posted event, so a press that
  follows its own release is not dropped by the OS.

Key events the tool sends itself are excluded from the stop key and hotkey
match, so a sequence that types `escape` does not stop itself. On macOS and
Windows this uses the synthetic-event flag; on Linux, which has no such flag,
recently sent keys are discounted by time.

## Development

```console
$ cargo test                     # unit tests: parser, keys, control, args
$ python3 tests/differential.py  # compare against the Swift original
```

`tests/differential.py` runs both programs in dry-run mode over
`tests/cases.txt` and compares the banner, error text and dry-run line. It needs
the Swift script; point it at a copy with `--swift PATH`.

```console
$ python3 tests/differential.py --swift ~/Downloads/auto-click.swift
differential: 51 agreed, 0 differed of 51
```

## Licence

MIT.
