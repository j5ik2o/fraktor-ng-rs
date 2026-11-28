#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Tests for scripts/check_pekko_differences.py.

Usage:
    uv run --with pyyaml python3 scripts/check_pekko_differences_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from check_pekko_differences import main  # noqa: E402

HEADER = "| 区分 | 話題 | 原典の行（Pekko v2.0.0） | fraktor の今の振る舞い | issue | 戻す段・入る PR | 状態 |"
SEPARATOR = "|:---|:---|:---|:---|:---|:---|:---|"


def row(category: str, issue: str, topic: str = "topic") -> str:
    return f"| {category} | {topic} | | | {issue} | | open |"


def differences(*rows: str) -> str:
    return "\n".join(["# 差の一覧", "", HEADER, SEPARATOR, *rows, ""])


def node(node_id: str, status: str, reason: str) -> str:
    return "\n".join(
        [
            f"- id: {node_id}",
            "  name: name",
            "  kind: test",
            f"  status: {status}",
            "  triage:",
            "    kind: pending",
            f"    reason: {reason}",
        ]
    )


def run_quietly(argv: list[str]) -> tuple[int, str, str]:
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        code = main(argv)
    return code, stdout.getvalue(), stderr.getvalue()


class CheckPekkoDifferencesTest(unittest.TestCase):
    def test_flags_ported_nodes_that_reference_temporary_issues(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            table = root / "differences.md"
            table.write_text(
                differences(
                    row("一時的（原典どおりに戻す）", "#311", "sender"),
                    row("環境の違い（JVM 固有。写していないのではない）", "#340", "rlimit"),
                    row("原典に定めが無い（fraktor の公開 API の選び）", "#350", "strategy"),
                ),
                encoding="utf-8",
            )
            ledger = root / "nodes.yaml"
            ledger.write_text(
                "\n".join(
                    [
                        "nodes:",
                        node("hash-ref-node", "green", "直すのは #311"),
                        node("issues-path-node", "green", "後続は https://github.com/j5ik2o/fraktor-ng-rs/issues/311 である"),
                        node("unported-node", "unported", "直すのは #311"),
                        node("environment-node", "green", "記録は #340"),
                        node("unrelated-node", "green", "直すのは #999"),
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            code, stdout, _ = run_quietly(["--differences", str(table), "--ledger", str(ledger)])

        self.assertEqual(code, 1)
        self.assertIn("食い違い: 2 件", stdout)
        self.assertIn("issue #311（sender）: hash-ref-node", stdout)
        self.assertIn("issue #311（sender）: issues-path-node", stdout)
        self.assertNotIn("unported-node", stdout)
        self.assertNotIn("environment-node", stdout)
        self.assertNotIn("unrelated-node", stdout)

    def test_malformed_table_exits_2(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            table = root / "differences.md"
            table.write_text(
                differences("| 一時的（原典どおりに戻す） | topic | | #311 | answer | open |"),
                encoding="utf-8",
            )
            ledger = root / "nodes.yaml"
            ledger.write_text("nodes: []\n", encoding="utf-8")

            code, _, stderr = run_quietly(["--differences", str(table), "--ledger", str(ledger)])

        self.assertEqual(code, 2)
        self.assertIn("欄が 7 個でない", stderr)


if __name__ == "__main__":
    unittest.main()
