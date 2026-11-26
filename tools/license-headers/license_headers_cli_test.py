"""#206 license_headers.py の CLI・Fail-first 原子性・外部ヘッダー・L3/L7 の結合テスト。"""
import io
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import header_core as core  # noqa: E402
import license_headers as lh  # noqa: E402
import notice_render as nr  # noqa: E402
import origin_decisions as od  # noqa: E402

ASF_AKKA = (
    "/*\n * Licensed to the Apache Software Foundation (ASF) under one or more\n"
    " * license agreements; and to You under the Apache License, version 2.0:\n *\n"
    " *   https://www.apache.org/licenses/LICENSE-2.0\n *\n"
    " * This file is part of the Apache Pekko project, which was derived from Akka.\n */\n\n"
    "/*\n * Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>\n */\n"
)
NETTY_BLOCK = (
    "\n/*\n * Copyright 2017 The Netty Project\n *\n"
    " * The Netty Project licenses this file to you under the Apache License,\n"
    " * version 2.0 (the \"License\"); you may not use this file except in compliance\n"
    " * with the License.\n */\n"
)
ASF_STD = (
    "/*\n * Licensed to the Apache Software Foundation (ASF) under one or more\n"
    " * contributor license agreements. See the NOTICE file distributed with\n"
    " * this work for additional information regarding copyright ownership.\n */\n"
)
DNS_SCALA = "actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala"
ID_SCALA = "actor/src/main/scala/org/apache/pekko/io/dns/IdGenerator.scala"
TSV_COLS = [
    "path", "判定", "根拠の種類", "根拠の中身", "要確認", "要確認の理由",
    "走査範囲", "付けるSPDX", "決定の出典", "原典ファイル", "第三者の表示", "表示のハッシュ",
]


def git(cwd, *args):
    return subprocess.run(["git", "-C", str(cwd), *args], check=True, capture_output=True, text=True).stdout.strip()


def init_git(path):
    git(path, "init", "-q")
    git(path, "config", "user.email", "test@example.com")
    git(path, "config", "user.name", "Test")


def run_cli(*args):
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = lh.main(list(args))
    return rc, out.getvalue(), err.getvalue()


def make_fake_pekko(root):
    pekko = root / "fake-pekko"
    pekko.mkdir()
    init_git(pekko)
    files = {
        DNS_SCALA: ASF_AKKA + NETTY_BLOCK + "package org.apache.pekko.io.dns\n",
        ID_SCALA: ASF_STD + "package org.apache.pekko.io.dns\n",
    }
    for rel, content in files.items():
        target = pekko / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
    git(pekko, "add", ".")
    git(pekko, "commit", "-q", "-m", "init")
    return pekko, git(pekko, "rev-parse", "HEAD")


def write_tsv(path, header, rows):
    lines = ["\t".join(header)] + ["\t".join(row) for row in rows]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


class CliWorkflowTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        init_git(self.repo)
        self.pekko, self.pekko_commit = make_fake_pekko(self.base)
        self._orig_commit = nr.PEKKO_COMMIT
        nr.PEKKO_COMMIT = self.pekko_commit

    def tearDown(self):
        nr.PEKKO_COMMIT = self._orig_commit
        self.tmp.cleanup()

    def add_file(self, rel, content):
        target = self.repo / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            target.write_bytes(content)
        else:
            target.write_text(content, encoding="utf-8")
        git(self.repo, "add", rel)
        return target

    def setup_standard_repo(self):
        self.add_file("crates/demo/src/dns.rs", "/// Mirrors `org.apache.pekko.io.dns.DnsSettings`.\nfn d() {}\n")
        self.add_file("crates/demo/src/id.rs", "//! Id generator\nfn id() {}\n")
        self.add_file("crates/demo/src/own.rs", "pub fn own() {}\n")
        self.add_file("crates/demo/src/pekko/sub.rs", "// inside crates/**/pekko/\nfn p() {}\n")
        self.add_file(
            "docs/design/issue-206-origin-decisions.tsv",
            "\t".join(od.DECISION_HEADER) + "\n@要確認\tApache-2.0\t元帥\t2026-11-26\t決定 2\n",
        )
        reader = nr.PekkoGitReader(self.pekko)
        h_dns = core.notice_hash(
            nr.render_pekko_notice_lines([DNS_SCALA], ["Copyright 2017 The Netty Project（DnsSettings.scala）"], reader.read_source, reader.third_party_blocks)
        )
        h_id = core.notice_hash(nr.render_pekko_notice_lines([ID_SCALA], (), reader.read_source, reader.third_party_blocks))
        h_def = core.notice_hash(core.DEFAULT_PEKKO_NOTICE_LINES)
        src_batch = "issue-206-origin-decisions.tsv:2（元帥 2026-11-26）"
        write_tsv(
            self.repo / "docs/design/issue-206-file-origin.tsv",
            TSV_COLS,
            [
                ["crates/demo/src/dns.rs", "Pekko 由来", "Mirrors", "m", "", "", "crates", "Apache-2.0", "判定", DNS_SCALA, "Copyright 2017 The Netty Project（DnsSettings.scala）", h_dns],
                ["crates/demo/src/id.rs", "Pekko 由来", "原典同名", "id", "要確認", "r", "crates", "Apache-2.0", src_batch, ID_SCALA, "", h_id],
                ["crates/demo/src/own.rs", "自分で書いた", "なし", "", "", "", "crates", "保留", "", "", "", ""],
                ["crates/demo/src/pekko/sub.rs", "Pekko 由来", "Pekko言及", "p", "要確認", "r", "crates", "Apache-2.0", src_batch, "", "", h_def],
            ],
        )
        git(self.repo, "add", "docs/design/issue-206-file-origin.tsv")

    def test_apply_manifest_list_post_write_idempotent_remove_and_l7(self):
        self.setup_standard_repo()
        # L7: 未追跡ファイルは走査されない。
        untracked = self.repo / "crates/demo/src/untracked.rs"
        untracked.write_text("fn u() {}\n", encoding="utf-8")

        rc, out, err = run_cli(
            "--root", str(self.repo), "--pekko", str(self.pekko),
            "--apply", "--manifest", "docs/design/issue-206-file-origin.tsv", "--list",
        )
        self.assertEqual(rc, 0, err)
        self.assertIn("added: 3  already headed: 0  held: 1", out)
        # --list は書き込んだ後の状態を出す（CodeRabbit 指摘の修正）。
        self.assertIn("apache crates/demo/src/dns.rs", out)
        self.assertIn("apache crates/demo/src/pekko/sub.rs", out)
        self.assertIn("held crates/demo/src/own.rs", out)
        # 保留と未追跡は 1 バイトも変わらない（I7, L7）。
        self.assertEqual((self.repo / "crates/demo/src/own.rs").read_text(encoding="utf-8"), "pub fn own() {}\n")
        self.assertEqual(untracked.read_text(encoding="utf-8"), "fn u() {}\n")
        # 原典の表示（Netty と ASF 標準形）が写っている。
        self.assertIn("Copyright 2017 The Netty Project", (self.repo / "crates/demo/src/dns.rs").read_text(encoding="utf-8"))
        id_text = (self.repo / "crates/demo/src/id.rs").read_text(encoding="utf-8")
        self.assertIn("contributor license agreements", id_text)
        self.assertNotIn("Lightbend", id_text)

        # 2 回目の --apply は冪等（added: 0）。
        rc2, out2, _ = run_cli(
            "--root", str(self.repo), "--pekko", str(self.pekko),
            "--apply", "--manifest", "docs/design/issue-206-file-origin.tsv",
        )
        self.assertEqual(rc2, 0)
        self.assertIn("added: 0  already headed: 3  held: 1", out2)

        # --check --manifest は合格（held: 1, errors: 0）。
        rc3, out3, _ = run_cli("--root", str(self.repo), "--check", "--manifest", "docs/design/issue-206-file-origin.tsv")
        self.assertEqual(rc3, 0, out3)
        self.assertIn("apache: 3", out3)
        self.assertIn("held: 1", out3)
        self.assertIn("errors: 0", out3)

        # F4: マーカーを書き換えても --remove で全件除去され、--list は除去後の missing を出す。
        dns_path = self.repo / "crates/demo/src/dns.rs"
        dns_path.write_text(dns_path.read_text(encoding="utf-8").replace("Mirrors", "Notes"), encoding="utf-8")
        rc4, out4, _ = run_cli("--root", str(self.repo), "--remove", "--list")
        self.assertEqual(rc4, 0)
        self.assertIn("removed: 3  without header: 1", out4)
        self.assertIn("missing crates/demo/src/dns.rs", out4)

    def test_apply_kind_l3_rejections_and_external_header_m3(self):
        self.setup_standard_repo()
        # L3: 決定行なしで --kind original は拒否。
        rc, _, err = run_cli("--root", str(self.repo), "--apply", "--kind", "original", "crates/demo/src/own.rs")
        self.assertEqual(rc, 1)
        self.assertIn("mit-without-decision", err)
        self.assertEqual((self.repo / "crates/demo/src/own.rs").read_text(encoding="utf-8"), "pub fn own() {}\n")

        # 決定行があれば --kind original が通る（ただし C3 マーカーがあるファイルは拒否）。
        dec = self.repo / "docs/design/issue-206-origin-decisions.tsv"
        dec.write_text(
            dec.read_text(encoding="utf-8")
            + "crates/demo/src/own.rs\tMIT OR Apache-2.0\t元帥\t2026-12-01\t照合済み\n"
            + "crates/demo/src/id.rs\tApache-2.0 AND Unlicense\t元帥\t2026-12-01\tM3\n",
            encoding="utf-8",
        )
        rc2, out2, err2 = run_cli(
            "--root", str(self.repo), "--apply", "--kind", "original", "--year", "2026", "crates/demo/src/own.rs"
        )
        self.assertEqual(rc2, 0, err2)
        self.assertIn("added: 1", out2)
        self.assertIn("SPDX-License-Identifier: MIT OR Apache-2.0", (self.repo / "crates/demo/src/own.rs").read_text(encoding="utf-8"))

        # M3: 外部ヘッダーは --remove で消さずに SKIP external を出す。
        id_path = self.repo / "crates/demo/src/id.rs"
        id_path.write_text(core.apply_header("fn id() {}\n", "external", spdx="Apache-2.0 AND Unlicense"), encoding="utf-8")
        rc3, out3, _ = run_cli("--root", str(self.repo), "--remove")
        self.assertEqual(rc3, 0, out3)
        self.assertIn("SKIP external crates/demo/src/id.rs", out3)
        self.assertIn("SPDX-License-Identifier: Apache-2.0 AND Unlicense", id_path.read_text(encoding="utf-8"))

    def test_i6_fail_first_atomicity_on_non_utf8_and_unrecognized(self):
        self.setup_standard_repo()
        # 非 UTF-8 ファイルがあると --apply --manifest は 1 件も書き込まず終了コード 1。
        bad = self.repo / "crates/demo/src/id.rs"
        bad.write_bytes(b"\xff\xfeinvalid")
        rc, _, err = run_cli(
            "--root", str(self.repo), "--pekko", str(self.pekko),
            "--apply", "--manifest", "docs/design/issue-206-file-origin.tsv",
        )
        self.assertEqual(rc, 1)
        self.assertIn("invalid-utf8", err)
        self.assertNotIn("SPDX-License-Identifier", (self.repo / "crates/demo/src/dns.rs").read_text(encoding="utf-8"))

        # --check もトレースバックなしで invalid-utf8 を返す（C4）。
        rc_chk, out_chk, _ = run_cli("--root", str(self.repo), "--check")
        self.assertEqual(rc_chk, 1)
        self.assertIn("invalid-utf8", out_chk)

        # --remove も未知 SPDX ブロックがあれば 1 件も消さずに終了コード 1（I6）。
        bad.write_text("// SPDX-License-Identifier: Apache-2.0\n// broken\n\nfn id() {}\n", encoding="utf-8")
        dns_path = self.repo / "crates/demo/src/dns.rs"
        headed_dns = core.apply_header("fn d() {}\n", "pekko")
        dns_path.write_text(headed_dns, encoding="utf-8")
        rc_rm, out_rm, _ = run_cli("--root", str(self.repo), "--remove")
        self.assertEqual(rc_rm, 1)
        self.assertIn("unrecognized-header", out_rm)
        self.assertEqual(dns_path.read_text(encoding="utf-8"), headed_dns)
