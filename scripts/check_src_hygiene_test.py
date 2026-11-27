#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Tests for scripts/check_src_hygiene.py.

Usage:
    python3 scripts/check_src_hygiene_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from check_src_hygiene import check_source, discover_targets, is_test_file, main  # noqa: E402


def rules_by_line(text: str) -> list[tuple[int, str]]:
    return [(violation.line, violation.rule) for violation in check_source("demo.rs", text)]


def run_quietly(argv: list[str]) -> int:
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return main(argv)


class ReviewCommentTest(unittest.TestCase):
    def test_flags_each_review_word_in_any_case(self) -> None:
        text = "\n".join(
            [
                "// Clean technical comment.",
                "/// Added after the design re-review.",
                "// Mirrors the spec (design of #234, section 3.2).",
                "// Matches item 441 of the audit.",
                "//! Re-Review in mixed case.",
                "// subitem 12 and items 3 are other words.",
            ]
        )
        self.assertEqual(
            rules_by_line(text),
            [(2, "review-comment"), (3, "review-comment"), (4, "review-comment"), (5, "review-comment")],
        )


class LetUnderscoreTest(unittest.TestCase):
    def test_flags_anonymous_bindings_and_allows_the_named_forms(self) -> None:
        text = "\n".join(
            [
                "fn f(_unused: &str) {",
                "  let _guard = lock.lock();",
                "  let _ignored = compute();",
                "  fallible().ok();",
                "  drop(value);",
                "  let _ = discard();",
                "  let _: usize = typed_discard();",
                "  let _=tight();",
                "}",
            ]
        )
        self.assertEqual(rules_by_line(text), [(6, "let-underscore"), (7, "let-underscore"), (8, "let-underscore")])

    def test_flags_a_doc_example_too(self) -> None:
        self.assertEqual(rules_by_line("/// let _ = x;"), [(1, "let-underscore")])


class TargetsTest(unittest.TestCase):
    def test_test_files_are_not_checked(self) -> None:
        for path in ["crates/a/src/tests.rs", "crates/a/src/foo_test.rs", "crates/a/src/foo_tests.rs", "crates/a/tests/spec.rs", "crates/a/src/x/tests/y.rs"]:
            self.assertTrue(is_test_file(Path(path)), path)
        for path in ["crates/a/src/test_kit.rs", "crates/a/src/tests_support.rs", "crates/a/src/lib.rs"]:
            self.assertFalse(is_test_file(Path(path)), path)

    def test_cli_passes_clean_code_and_fails_on_an_injected_word_and_let(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "tests" / "repo"  # a `tests` above the root must not exclude anything
            src = root / "crates" / "demo" / "src"
            src.mkdir(parents=True)
            (src / "lib.rs").write_text("pub fn ok(_x: i32) {}\n", encoding="utf-8")
            (src / "lib_test.rs").write_text("// design of #999\nfn t() { let _ = 1; }\n", encoding="utf-8")
            self.assertEqual(discover_targets(root, []), [src / "lib.rs"])
            self.assertEqual(run_quietly(["--root", str(root)]), 0)

            (src / "bad.rs").write_text("// re-review item 441\npub fn f() { let _ = 1; }\n", encoding="utf-8")
            self.assertEqual(run_quietly(["--root", str(root)]), 1)
            self.assertEqual(run_quietly(["--root", str(root), "crates/demo/src/lib.rs"]), 0)
            self.assertEqual(run_quietly(["--root", str(root), "crates/nowhere"]), 2)


if __name__ == "__main__":
    unittest.main()
