#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Forbids review-process wording and `let _ =` in the non-test code of crates/*/src (#279).

Rules, applied to every line of every checked file, comments and doc
comments included:
  review-comment  `re-review`, `design of #`, or `item <number>`, in any case.
                  The review history belongs in the design documents and the
                  PR, not in the code.
  let-underscore  `let _ =` or `let _: T =`. Drop the value with a plain
                  statement, `.ok()`, or `drop(..)`; prefix an unused
                  parameter with `_`; keep a guard alive with a named
                  binding such as `let _guard =`.

Checked files: crates/*/src/**/*.rs, except test files: `tests.rs`,
`*_test.rs`, `*_tests.rs`, and any file under a `tests/` directory.
There is no suppression comment; a change to the rules is the owner's call.

Usage:
    python3 scripts/check_src_hygiene.py [--root DIR] [PATH ...]
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

RULES = (
    ("review-comment", re.compile(r"re-review|design of #|\bitem [0-9]+\b", re.IGNORECASE)),
    ("let-underscore", re.compile(r"\blet\s+_\s*[=:]")),
)


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    rule: str
    snippet: str


def is_test_file(relative: Path) -> bool:
    """True for a test file, which the rules do not cover."""
    name = relative.name
    return name == "tests.rs" or name.endswith(("_test.rs", "_tests.rs")) or "tests" in relative.parts[:-1]


def check_source(display_path: str, text: str) -> list[Violation]:
    """Returns the violations of one file's text, in line order."""
    return [
        Violation(display_path, number, rule, line.strip())
        for number, line in enumerate(text.splitlines(), start=1)
        for rule, pattern in RULES
        if pattern.search(line)
    ]


def discover_targets(root: Path, paths: list[str]) -> list[Path]:
    """The non-test .rs files under `paths` (relative to `root`), or under crates/*/src."""
    if paths:
        bases = [root / raw for raw in paths]
        missing = [str(base) for base in bases if not base.exists()]
        if missing:
            raise FileNotFoundError(", ".join(missing))
    else:
        bases = sorted((root / "crates").glob("*/src"))
    files = {file for base in bases for file in ([base] if base.is_file() else base.rglob("*.rs"))}
    return sorted(file for file in files if file.suffix == ".rs" and not is_test_file(file.relative_to(root)))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check crates/*/src for review-process wording and `let _ =`.")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1], help="repository root")
    parser.add_argument("paths", nargs="*", help="files or directories under the root to check instead of crates/*/src")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        targets = discover_targets(root, args.paths)
    except FileNotFoundError as error:
        print(f"error: path not found: {error}", file=sys.stderr)
        return 2

    violations = [
        violation
        for target in targets
        for violation in check_source(target.relative_to(root).as_posix(), target.read_text(encoding="utf-8"))
    ]
    for violation in violations:
        print(f"{violation.path}:{violation.line}: [{violation.rule}] {violation.snippet}", file=sys.stderr)
    print(f"src hygiene: checked {len(targets)} file(s), {len(violations)} violation(s)")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
