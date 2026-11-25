#!/usr/bin/env python3
"""apply_ported_headers.py の冪等性と対象範囲を、一時ディレクトリで確かめる。

使い方:
    python3 tools/license-headers/apply_ported_headers_test.py

確かめること:
  1. dry-run はファイルを書かず、対象件数を出す
  2. --apply は移植マーカーのあるファイルだけにヘッダーを付ける
  3. --apply を 2 回しても内容が変わらない（冪等）
  4. --remove は元の内容に戻す
  5. --check は未適用なら 1、適用後は 0 を返す
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("apply_ported_headers.py")

PORTED = "// Ported from Pekko v2.0.0 (SHA abc):\n// original/path.scala\n\nfn ported() {}\n"
CHECKED = "// Checked against Pekko v2.0.0 (SHA abc).\n\nfn checked() {}\n"
PLAIN = "fn plain() {}\n"


class ApplyPortedHeadersTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        tests = self.root / "crates" / "demo" / "tests"
        src = self.root / "crates" / "demo" / "src"
        tests.mkdir(parents=True)
        src.mkdir(parents=True)
        self.ported = tests / "ported_spec.rs"
        self.checked = tests / "checked_spec.rs"
        self.plain = src / "plain.rs"
        self.ported.write_text(PORTED, encoding="utf-8")
        self.checked.write_text(CHECKED, encoding="utf-8")
        self.plain.write_text(PLAIN, encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def run_script(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root), *args],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_dry_run_counts_without_writing(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0)
        self.assertIn("ported: 2", result.stdout)
        self.assertIn("to add: 2", result.stdout)
        self.assertEqual(self.ported.read_text(encoding="utf-8"), PORTED)
        self.assertEqual(self.checked.read_text(encoding="utf-8"), CHECKED)

    def test_apply_is_idempotent_and_skips_unmarked(self):
        first = self.run_script("--apply")
        self.assertEqual(first.returncode, 0)
        after_first = self.ported.read_text(encoding="utf-8")
        self.assertTrue(after_first.startswith("// SPDX-License-Identifier: Apache-2.0"))
        self.assertTrue(after_first.endswith(PORTED))
        self.assertEqual(self.checked.read_text(encoding="utf-8").count("SPDX-License-Identifier"), 1)
        self.assertEqual(self.plain.read_text(encoding="utf-8"), PLAIN)

        second = self.run_script("--apply")
        self.assertEqual(second.returncode, 0)
        self.assertIn("added: 0", second.stdout)
        self.assertEqual(self.ported.read_text(encoding="utf-8"), after_first)

    def test_remove_restores_original(self):
        self.run_script("--apply")
        result = self.run_script("--remove")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(self.ported.read_text(encoding="utf-8"), PORTED)
        self.assertEqual(self.checked.read_text(encoding="utf-8"), CHECKED)

    def test_check_reports_pending_then_clean(self):
        self.assertEqual(self.run_script("--check").returncode, 1)
        self.run_script("--apply")
        clean = self.run_script("--check")
        self.assertEqual(clean.returncode, 0)
        self.assertIn("pending: 0", clean.stdout)


if __name__ == "__main__":
    unittest.main()
