#!/usr/bin/env python3
"""Differential test: the Swift original and the Rust port must agree.

Every case is run through both programs in dry-run mode, which prints the
parsed sequence and sends no input. The banner, error text and dry-run line are
compared after normalising the few lines that legitimately differ (the platform
note, the permission warning, and stdout/stderr interleaving).

Usage:
    python3 tests/differential.py [--swift PATH] [--rust PATH] [--cases PATH]

Exits 0 when every case agrees.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

HOME = Path.home()
DEFAULT_SWIFT = HOME / "Downloads" / "auto-click.swift"
DEFAULT_RUST = HOME / "code" / "clickloop" / "target" / "release" / "clickloop"
DEFAULT_CASES = Path(__file__).parent / "cases.txt"

# Lines that are allowed to differ between the two implementations.
DROP = re.compile(
    r"^(Starting in|macOS:|Windows:|Linux:|dry run:|Warning:|Stopped: |"
    r"Press .* to stop at any time)"
)


def normalise(text: str) -> list[str]:
    """The comparable lines of a run, order-insensitive."""
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line or DROP.match(line):
            continue
        # The dry-run marker differs only in punctuation.
        line = re.sub(r"^\[dry-run\]\s*", "", line)
        # Long descriptions end with a total count; keep the shape, not the digits.
        line = re.sub(r"\(\d+ actions total\)", "(N actions total)", line)
        # Waits are printed with their seconds; the wording is what matters.
        line = re.sub(r"wait [0-9.]+s", "wait Ns", line)
        out.append(line)
    return sorted(out)


def run(cmd: list[str]) -> str:
    result = subprocess.run(
        cmd, capture_output=True, text=True, timeout=120, check=False
    )
    return result.stdout + result.stderr


def load_cases(path: Path) -> list[tuple[str, list[str]]]:
    cases = []
    for raw in path.read_text().splitlines():
        line = raw.rstrip()
        # Blank lines and comments are skipped; a comment may follow a tab too.
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if "\t" in line:
            actions, extra = line.split("\t", 1)
            extra_args = extra.split()
        else:
            actions, extra_args = line, []
        cases.append((actions, extra_args))
    return cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--swift", default=str(DEFAULT_SWIFT))
    parser.add_argument("--rust", default=str(DEFAULT_RUST))
    parser.add_argument("--cases", default=str(DEFAULT_CASES))
    args = parser.parse_args()

    swift_src = Path(args.swift)
    rust_bin = Path(args.rust)
    for path, what in ((swift_src, "Swift source"), (rust_bin, "Rust binary")):
        if not path.exists():
            print(f"missing {what}: {path}", file=sys.stderr)
            return 2

    # A single cycle, no start delay and no time limit, so each program prints
    # exactly one dry-run line no matter how fast it starts.
    common = ["--dry-run", "--start-delay", "0", "--duration", "0", "--count", "1"]

    cases = load_cases(Path(args.cases))
    if not cases:
        print("no cases found", file=sys.stderr)
        return 2

    agreed = 0
    differed: list[str] = []
    for actions, extra in cases:
        swift_cmd = ["swift", str(swift_src), "--actions", actions, *common]
        rust_cmd = [str(rust_bin), "--actions", actions, *common]
        # Extra args come last so they override the harness defaults where the
        # case intends to (a case testing --count overrides the single cycle).
        swift_cmd += extra
        rust_cmd += extra

        swift_out = normalise(run(swift_cmd))
        rust_out = normalise(run(rust_cmd))

        if swift_out == rust_out:
            agreed += 1
            continue

        differed.append(actions)
        print(f"MISMATCH: {actions!r}" + (f" with {extra}" if extra else ""))
        for line in sorted(set(swift_out) - set(rust_out)):
            print(f"    only swift: {line}")
        for line in sorted(set(rust_out) - set(swift_out)):
            print(f"    only rust:  {line}")
        print()

    print(f"differential: {agreed} agreed, {len(differed)} differed of {len(cases)}")
    return 0 if not differed else 1


if __name__ == "__main__":
    sys.exit(main())
