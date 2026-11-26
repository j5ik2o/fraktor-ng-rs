#!/usr/bin/env python3
"""#206 license_headers.py の総合テスト（F1〜F8・PBT・CLI・C1〜C6・レビュー試作の壊し方 M-a〜M-o）。

使い方:
    python3 tools/license-headers/license_headers_test.py
"""
import hashlib
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import header_core as core  # noqa: E402
import license_headers_cli_test as cli  # noqa: E402
import license_headers_pbt as pbt  # noqa: E402
import notice_render as nr  # noqa: E402
import origin_decisions as od  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
APACHE_201_SHA256 = "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4"
PEKKO_NOTICE_SHA256 = "8188a337ad167b8bb3a2b53868a061130aca0bcc8abff8fd717900cdb444e73e"
HASH_PROSPECTOR_SHA256 = "591ad911e4078fc34f22ed9ca5467817e5d2572ee6655dd4f98fc428a6bd6ebc"
CAFFEINE_SHA256 = "68d67061f0d3de1a201a90c9be5df803496a5297a73ef2a9a69875b026dd4d8b"


class ReviewTamperingAndC6Test(unittest.TestCase):
    """設計レビュー 3 回目・4 回目の試作 M-a〜M-o と C6（§3.4.4 の 6 テスト案）の検証。"""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        cli.init_git(self.repo)
        self.pekko, self.pekko_commit = cli.make_fake_pekko(self.base)
        self._orig_commit = nr.PEKKO_COMMIT
        nr.PEKKO_COMMIT = self.pekko_commit
        self._build_stage_a_repo()

    def tearDown(self):
        nr.PEKKO_COMMIT = self._orig_commit
        self.tmp.cleanup()

    def _add(self, rel, text):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        cli.git(self.repo, "add", rel)
        return path

    def _build_stage_a_repo(self):
        if self.repo.exists():
            shutil.rmtree(self.repo)
        self.repo.mkdir()
        cli.init_git(self.repo)
        self._add("crates/a/src/ask_driver.rs", "// Pekko mention\nfn ask() {}\n")
        self._add("crates/a/tests/ported_spec.rs", "// Ported from Pekko v2.0.0\nfn t() {}\n")
        self._add("crates/a/src/ask_future.rs", "// filler\n" * 130 + "/// Mirrors `org.apache.pekko.pattern.ask`.\nfn f() {}\n")
        self._add("crates/a/src/dns_settings.rs", "/// Mirrors `org.apache.pekko.io.dns.DnsSettings`.\nfn d() {}\n")
        self._add(
            "crates/a/src/fast_frequency_sketch.rs",
            "// A low-bias hash function found by Hash Function Prospector\n"
            "// https://github.com/skeeto/hash-prospector\n"
            "// hash-prospector has been placed in public domain - https://github.com/skeeto/hash-prospector/blob/master/UNLICENSE\n"
            "fn rehash(x: i32) -> i32 { x }\n",
        )
        self._add("crates/a/src/own.rs", "pub fn own() {}\n")
        self._add("docs/note.md", "# Note mentioning Pekko\n")
        self._add(
            "docs/design/issue-206-origin-decisions.tsv",
            "\t".join(od.DECISION_HEADER) + "\n@要確認\tApache-2.0\t元帥\t2026-11-26\t決定 2\n",
        )
        reader = nr.PekkoGitReader(self.pekko)
        h_def = core.notice_hash(core.DEFAULT_PEKKO_NOTICE_LINES)
        h_dns = core.notice_hash(
            nr.render_pekko_notice_lines(
                [cli.DNS_SCALA], ["Copyright 2017 The Netty Project（DnsSettings.scala）"],
                reader.read_source, reader.third_party_blocks,
            )
        )
        batch = "issue-206-origin-decisions.tsv:2（元帥 2026-11-26）"
        self.rows = [
            ["crates/a/src/ask_driver.rs", "Pekko 由来", "Pekko言及", "p", "要確認", "r", "crates", "Apache-2.0", batch, "", "", h_def],
            ["crates/a/tests/ported_spec.rs", "Pekko 由来", "移植マーカー", "m", "", "", "crates", "Apache-2.0", "判定", "", "", h_def],
            ["crates/a/src/ask_future.rs", "Pekko 由来", "Mirrors", "m", "", "", "crates", "Apache-2.0", "判定", "", "", h_def],
            ["crates/a/src/dns_settings.rs", "Pekko 由来", "Mirrors", "m", "", "", "crates", "Apache-2.0", "判定", cli.DNS_SCALA, "Copyright 2017 The Netty Project（DnsSettings.scala）", h_dns],
            ["crates/a/src/fast_frequency_sketch.rs", "Pekko 由来", "第三者のコードへの言及", "h", "要確認", "r", "crates", "Apache-2.0", batch, "", core.HASH_PROSPECTOR_LABEL, h_def],
            ["crates/a/src/own.rs", "自分で書いた", "なし", "", "", "", "crates", "保留", "", "", "", ""],
            ["docs/note.md", "Pekko 由来", "Pekko言及", "p", "要確認", "r", "other", "Apache-2.0", batch, "", "", ""],
        ]
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, self.rows)
        cli.git(self.repo, "add", "docs/design/issue-206-file-origin.tsv")
        rc, _, err = cli.run_cli(
            "--root", str(self.repo), "--pekko", str(self.pekko),
            "--apply", "--manifest", "docs/design/issue-206-file-origin.tsv",
        )
        self.assertEqual(rc, 0, err)

    def _check(self):
        return cli.run_cli("--root", str(self.repo), "--check", "--manifest", "docs/design/issue-206-file-origin.tsv")

    def _replace_with_dual(self, rel):
        path = self.repo / rel
        body = core.remove_header(path.read_text(encoding="utf-8"))[0]
        dual = "\n".join(core.build_original_header_lines(2026)) + "\n\n" + body
        path.write_text(dual, encoding="utf-8")

    def test_baseline_passes(self):
        rc, out, _ = self._check()
        self.assertEqual(rc, 0, out)
        self.assertIn("apache: 5", out)
        self.assertIn("held: 1", out)
        self.assertIn("errors: 0", out)

    def test_m_a_m_b_m_c_tampering_on_flagged_file(self):
        # M-a: 要確認ファイルに MIT。
        self._replace_with_dual("crates/a/src/ask_driver.rs")
        rc, out, _ = self._check()
        self.assertEqual(rc, 1)
        self.assertIn("mit-without-decision", out)
        self.assertIn("manifest-mismatch", out)

        # M-b: M-a + TSV の 付けるSPDX セルを手で書き換え。
        rows = [list(r) for r in self.rows]
        rows[0][7] = core.SPDX_DUAL
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, rows)
        rc_b, out_b, _ = self._check()
        self.assertEqual(rc_b, 1)
        self.assertIn("manifest-tampered", out_b)

        # M-c: M-a + TSV から行を消す。
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, self.rows[1:])
        rc_c, out_c, _ = self._check()
        self.assertEqual(rc_c, 1)
        self.assertIn("new-file-not-apache", out_c)
        self.assertIn("mit-without-decision", out_c)

    def test_m_d_m_e_m_f_m_g_m_h_m_i_m_j_m_o(self):
        # M-d: 移植マーカーの行を消して MIT -> C3 + new-file-not-apache。
        self._replace_with_dual("crates/a/tests/ported_spec.rs")
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, [r for r in self.rows if r[0] != "crates/a/tests/ported_spec.rs"])
        rc_d, out_d, _ = self._check()
        self.assertEqual(rc_d, 1)
        self.assertIn("origin-conflict", out_d)

        # M-e: 130 行目 Mirrors の行を消して MIT -> C3 がファイル全体を見て落とす。
        self._build_stage_a_repo()
        self._replace_with_dual("crates/a/src/ask_future.rs")
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, [r for r in self.rows if r[0] != "crates/a/src/ask_future.rs"])
        rc_e, out_e, _ = self._check()
        self.assertEqual(rc_e, 1)
        self.assertIn("origin-conflict", out_e)

        # M-f & M-o: 新しいパスへコピーして MIT / 外部式 -> new-file-not-apache。
        self._build_stage_a_repo()
        self._add("crates/a/src/copy_mit.rs", core.apply_header("fn x() {}\n", "original", year=2026))
        self._add("crates/a/src/copy_ext.rs", core.apply_header("fn y() {}\n", "external", spdx="Apache-2.0 AND Unlicense"))
        rc_f, out_f, _ = self._check()
        self.assertEqual(rc_f, 1)
        self.assertIn("new-file-not-apache crates/a/src/copy_mit.rs", out_f)
        self.assertIn("new-file-not-apache crates/a/src/copy_ext.rs", out_f)

        # M-g & M-h: 保留ファイルに MIT、文書（other）に MIT。
        self._build_stage_a_repo()
        self._replace_with_dual("crates/a/src/own.rs")
        (self.repo / "docs/note.md").write_text("<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->\n# Note\n", encoding="utf-8")
        rc_gh, out_gh, _ = self._check()
        self.assertEqual(rc_gh, 1)
        self.assertIn("held-has-spdx crates/a/src/own.rs", out_gh)
        self.assertIn("mit-without-decision docs/note.md", out_gh)

        # M-i & M-j: git mv して MIT（行あり -> manifest-missing-file、行消し -> new-file-not-apache）。
        self._build_stage_a_repo()
        cli.git(self.repo, "mv", "crates/a/src/ask_driver.rs", "crates/a/src/moved_ask.rs")
        self._replace_with_dual("crates/a/src/moved_ask.rs")
        rc_i, out_i, _ = self._check()
        self.assertEqual(rc_i, 1)
        self.assertIn("manifest-missing-file crates/a/src/ask_driver.rs", out_i)
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, self.rows[1:])
        rc_j, out_j, _ = self._check()
        self.assertEqual(rc_j, 1)
        self.assertIn("new-file-not-apache crates/a/src/moved_ask.rs", out_j)

    def test_m_k_m_k2_m_l_and_m_m_valid_path(self):
        # M-k2: ヘッダーだけを Apache-2.0 OR MIT に改変。
        p = self.repo / "crates/a/tests/ported_spec.rs"
        p.write_text(p.read_text(encoding="utf-8").replace("Apache-2.0", "Apache-2.0 OR MIT", 1), encoding="utf-8")
        rc_k2, out_k2, _ = self._check()
        self.assertEqual(rc_k2, 1)
        self.assertIn("spdx-grammar", out_k2)
        self.assertIn("mit-without-decision", out_k2)
        self.assertIn("origin-conflict", out_k2)

        # M-k: 決定のファイルに Apache-2.0 OR MIT を書くと読込段で止まる。
        self._build_stage_a_repo()
        dec = self.repo / "docs/design/issue-206-origin-decisions.tsv"
        dec.write_text(dec.read_text(encoding="utf-8") + "crates/a/src/own.rs\tApache-2.0 OR MIT\t元帥\t2026-12-01\tx\n", encoding="utf-8")
        rc_k, out_k, _ = self._check()
        self.assertEqual(rc_k, 1)
        self.assertIn("ERROR decisions", out_k)

        # M-l: 決定の行を足したが TSV 未更新 -> manifest-tampered。
        self._build_stage_a_repo()
        dec = self.repo / "docs/design/issue-206-origin-decisions.tsv"
        dec.write_text(dec.read_text(encoding="utf-8") + "crates/a/src/own.rs\tMIT OR Apache-2.0\t元帥\t2026-12-01\t照合済み\n", encoding="utf-8")
        rc_l, out_l, _ = self._check()
        self.assertEqual(rc_l, 1)
        self.assertIn("manifest-tampered crates/a/src/own.rs", out_l)

        # M-m: 正しい道（弱い根拠・保留に決定行＋TSV 揃え＋ヘッダー付与）は通る。
        rows = [list(r) for r in self.rows]
        rows[5][7] = core.SPDX_DUAL
        rows[5][8] = "issue-206-origin-decisions.tsv:3（元帥 2026-12-01）"
        cli.write_tsv(self.repo / "docs/design/issue-206-file-origin.tsv", cli.TSV_COLS, rows)
        self._replace_with_dual("crates/a/src/own.rs")
        rc_m, out_m, _ = self._check()
        self.assertEqual(rc_m, 0, out_m)
        self.assertIn("dual: 1", out_m)

    def test_c6_all_six_cases_and_exact_hashed_slice(self):
        # (1) 適用直後は合格（ハッシュ対象は先頭 2 行と固定末尾 5 行を除いた行の並び）。
        dns = self.repo / "crates/a/src/dns_settings.rs"
        parsed = core.parse_header(dns.read_text(encoding="utf-8"))
        self.assertEqual(parsed.header_lines[0], "// SPDX-License-Identifier: Apache-2.0")
        self.assertEqual(parsed.header_lines[1], "//")
        self.assertEqual(parsed.header_lines[-5:], core.PEKKO_TRAILER_LINES)
        self.assertEqual(core.notice_hash(parsed.header_lines[2:-5]), self.rows[3][11])
        self.assertEqual(self._check()[0], 0)

        # (2) Netty ブロックを削除 -> notice-missing + notice-mismatch。
        orig_dns = dns.read_text(encoding="utf-8")
        no_netty = "\n".join(line for line in orig_dns.splitlines() if "Netty" not in line and "version 2.0 (the" not in line and "with the License." not in line) + "\n"
        dns.write_text(no_netty, encoding="utf-8")
        rc2, out2, _ = self._check()
        self.assertEqual(rc2, 1)
        self.assertIn("notice-missing crates/a/src/dns_settings.rs", out2)
        self.assertIn("notice-mismatch crates/a/src/dns_settings.rs", out2)

        # (4) ヘッダーの外（本文）に Netty の行を書いても合格にならない。
        dns.write_text(no_netty + "// Copyright 2017 The Netty Project\n", encoding="utf-8")
        rc4, out4, _ = self._check()
        self.assertEqual(rc4, 1)
        self.assertIn("notice-missing crates/a/src/dns_settings.rs", out4)

        # (3) Lightbend の年を 2018-2022 -> 2018-2023 に変えると notice-mismatch。
        dns.write_text(orig_dns.replace("2018-2022", "2018-2023"), encoding="utf-8")
        rc3, out3, _ = self._check()
        self.assertEqual(rc3, 1)
        self.assertIn("notice-mismatch crates/a/src/dns_settings.rs", out3)
        dns.write_text(orig_dns, encoding="utf-8")

        # (5) 原典ファイルが決まらない行（既定表示）で ASF の表示を消すと落ちる。
        ask = self.repo / "crates/a/src/ask_driver.rs"
        orig_ask = ask.read_text(encoding="utf-8")
        ask.write_text(orig_ask.replace("Licensed to the Apache Software Foundation (ASF) under one or more\n", ""), encoding="utf-8")
        rc5, out5, _ = self._check()
        self.assertEqual(rc5, 1)
        self.assertIn("missing-required-notice crates/a/src/ask_driver.rs", out5)
        self.assertIn("notice-mismatch crates/a/src/ask_driver.rs", out5)
        ask.write_text(orig_ask, encoding="utf-8")

        # (6) fast_frequency_sketch.rs で hash-prospector コメントを消すと notice-missing。
        ffs = self.repo / "crates/a/src/fast_frequency_sketch.rs"
        ffs.write_text(ffs.read_text(encoding="utf-8").replace("hash-prospector", "removed"), encoding="utf-8")
        rc6, out6, _ = self._check()
        self.assertEqual(rc6, 1)
        self.assertIn("notice-missing crates/a/src/fast_frequency_sketch.rs", out6)


class RootLicenseFilesTest(unittest.TestCase):
    """ルートの LICENSE-APACHE・NOTICE の整合性検証（§2.1, §2.4, §8.1 段 A）。"""

    def test_root_license_apache_and_no_legacy_license_or_mit(self):
        apache = REPO_ROOT / "LICENSE-APACHE"
        self.assertTrue(apache.is_file(), "LICENSE-APACHE must exist at repo root")
        self.assertFalse((REPO_ROOT / "LICENSE").exists(), "legacy LICENSE must be removed")
        self.assertFalse((REPO_ROOT / "LICENSE-MIT").exists(), "LICENSE-MIT must not exist in Stage A")
        data = apache.read_bytes()
        self.assertEqual(len(data.splitlines()), 201)
        self.assertEqual(hashlib.sha256(data).hexdigest(), APACHE_201_SHA256)

    def test_root_notice_contains_verbatim_pekko_notice_and_two_license_sections(self):
        notice_path = REPO_ROOT / "NOTICE"
        self.assertTrue(notice_path.is_file())
        raw = notice_path.read_text(encoding="utf-8")
        divider = "=" * 72
        parts = raw.split(f"\n{divider}\n\n")
        self.assertEqual(len(parts), 3, "NOTICE must have two 72-char '=' dividers")
        upstream_notice = parts[1].removesuffix("\n") + "\n"
        self.assertEqual(hashlib.sha256(upstream_notice.encode("utf-8")).hexdigest(), PEKKO_NOTICE_SHA256)

        appendix_lines = parts[2].splitlines(keepends=True)
        # 2 行の前置き + 空行 1 行の後に hash-prospector（28 行）、空行 1 行、Caffeine（3 行）。
        self.assertTrue(appendix_lines[0].startswith("The following sections are reproduced unmodified"))
        self.assertEqual(appendix_lines[2], "\n")
        hp_bytes = "".join(appendix_lines[3:31]).encode("utf-8")
        self.assertEqual(appendix_lines[31], "\n")
        caf_bytes = "".join(appendix_lines[32:35]).encode("utf-8")
        self.assertEqual(hashlib.sha256(hp_bytes).hexdigest(), HASH_PROSPECTOR_SHA256)
        self.assertEqual(hashlib.sha256(caf_bytes).hexdigest(), CAFFEINE_SHA256)


def load_tests(loader, tests, _pattern):
    suite = unittest.TestSuite([tests])
    suite.addTests(loader.loadTestsFromModule(pbt))
    suite.addTests(loader.loadTestsFromModule(cli))
    return suite


if __name__ == "__main__":
    unittest.main()
