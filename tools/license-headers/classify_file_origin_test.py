"""#206 の線引き（classify_file_origin.py と origin_*.py）の H1 の規則、人の決定の重ね方、原典の表示の切り出しのテスト。

原典の git リポジトリは要らない（型の索引は小さな辞書で与える）。
    python3 tools/license-headers/classify_file_origin_test.py
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import classify_file_origin as c  # noqa: E402
import origin_decisions as d  # noqa: E402
import origin_evidence as e  # noqa: E402
import origin_pekko as p  # noqa: E402

TYPES = {
    "AddressFromURIString": ["actor/src/main/scala/org/apache/pekko/actor/Address.scala"],
    "TestActors": ["testkit/src/main/scala/org/apache/pekko/testkit/TestActors.scala"],
    "UnrestrictedStash": ["actor/src/main/scala/org/apache/pekko/actor/Stash.scala"],
}
FOLDED = p.fold_types(TYPES)
PEKKO = p.PekkoIndex(paths=frozenset(), stems={}, types=TYPES, folded=FOLDED, notices={})


def row(path, verdict="自分で書いた", kind="なし", flag="", scope="crates"):
    return (path, scope, verdict, kind, "", flag, "")


class H1Rules(unittest.TestCase):
    def test_rule1_matches_type_names_ignoring_case(self):
        text = "pub struct AddressFromUriString;\n"
        self.assertIsNone(e.type_evidence(text, TYPES))
        self.assertIn("Address.scala", e.type_evidence_folded(text, FOLDED))

    def test_rule2_matches_scala_type_method_in_comments(self):
        text = "/// Returns the recipe, corresponding to\n/// `TestActors.echoActorProps`.\nfn f() {}\n"
        self.assertIn("TestActors.echoActorProps", e.type_method_evidence(text, FOLDED))

    def test_rule2_ignores_snake_case_and_code_lines(self):
        self.assertIsNone(e.type_method_evidence("// `TestActors.echo_actor_props`\n", FOLDED))
        self.assertIsNone(e.type_method_evidence("let x = TestActors.echoActorProps;\n", FOLDED))
        self.assertIsNone(e.type_method_evidence("// `Duration.fromSecs`\n", FOLDED))

    def test_rule3_phrases(self):
        for line in (
            "// They complement the ported case in tests/x.rs.",
            "// corresponding to the spec.",
            "//! same original `TestKit` contracts",
        ):
            self.assertIsNotNone(e.ported_words_evidence(line + "\n"), line)

    def test_rule3_finds_the_original_across_a_line_break(self):
        text = "// Zero rows is allowed, as in the\n// original: such a sketch counts nothing.\nfn f() {}\n"
        self.assertEqual(e.ported_words_evidence(text), "// Zero rows is allowed, as in the")

    def test_rule3_does_not_join_across_code(self):
        text = "// as in the\nfn f() {}\n// original: x\n"
        self.assertIsNone(e.ported_words_evidence(text))

    def test_rule4_third_party_code(self):
        self.assertIsNotNone(e.third_party_text_evidence("// with the steps of `scala.util.hashing.MurmurHash3`.\n"))
        self.assertIsNotNone(e.third_party_text_evidence("// expected `MurmurHash3.stringHash(v, 1)`\n"))
        self.assertIsNone(e.third_party_text_evidence("// a MurmurHash3 hash\n"))

    def test_classify_marks_h1_hits_as_flagged_pekko(self):
        text = "// as `UnrestrictedStash.postStop` does.\nfn f() {}\n"
        verdict = e.classify("crates/a/src/x_test.rs", text, "crates", {}, PEKKO)
        self.assertEqual(verdict[0], "Pekko 由来")
        self.assertEqual(verdict[3], "要確認")


class Decisions(unittest.TestCase):
    ROWS = [
        row("crates/a/src/own.rs"),
        row("crates/a/src/flagged.rs", verdict="Pekko 由来", kind="原典同名", flag="要確認"),
        row("crates/a/src/ported.rs", verdict="Pekko 由来", kind="移植マーカー"),
        row("docs/x.md", verdict="Pekko 由来", kind="Pekko言及", flag="要確認", scope="other"),
    ]

    def decide(self, decisions):
        return d.apply_decisions(self.ROWS, [(n + 2, *d) for n, d in enumerate(decisions)], "d.tsv")

    def test_without_decisions_only_confirmed_pekko_gets_apache(self):
        out = self.decide([])
        self.assertEqual(out["crates/a/src/ported.rs"][0], d.SPDX_APACHE)
        self.assertEqual(out["crates/a/src/own.rs"][0], d.HELD)
        self.assertEqual(out["crates/a/src/flagged.rs"][0], d.HELD)

    def test_batch_decision_covers_flagged_rows_only(self):
        out = self.decide([("@要確認", d.SPDX_APACHE, "元帥", "2026-11-26", "決定 2")])
        self.assertEqual(out["crates/a/src/flagged.rs"][0], d.SPDX_APACHE)
        self.assertEqual(out["docs/x.md"][0], d.SPDX_APACHE)
        self.assertEqual(out["crates/a/src/own.rs"][0], d.HELD)

    def test_mit_needs_a_per_file_decision_and_wins_over_the_batch(self):
        out = self.decide([
            ("@要確認", d.SPDX_APACHE, "元帥", "2026-11-26", "決定 2"),
            ("crates/a/src/flagged.rs", d.SPDX_DUAL, "元帥", "2026-12-01", "照合済み"),
            ("crates/a/src/own.rs", d.SPDX_DUAL, "元帥", "2026-12-01", "照合済み"),
        ])
        self.assertEqual(out["crates/a/src/flagged.rs"][0], d.SPDX_DUAL)
        self.assertEqual(out["crates/a/src/own.rs"][0], d.SPDX_DUAL)

    def test_refuses_mit_in_batch_or_on_strong_evidence(self):
        with self.assertRaises(SystemExit):
            self.decide([("@要確認", d.SPDX_DUAL, "元帥", "2026-11-26", "x")])
        with self.assertRaises(SystemExit):
            self.decide([("crates/a/src/ported.rs", d.SPDX_DUAL, "元帥", "2026-11-26", "x")])

    def test_refuses_unknown_paths_and_duplicates(self):
        with self.assertRaises(SystemExit):
            self.decide([("crates/a/src/missing.rs", d.SPDX_APACHE, "元帥", "2026-11-26", "x")])
        with self.assertRaises(SystemExit):
            self.decide([
                ("crates/a/src/own.rs", d.SPDX_APACHE, "元帥", "2026-11-26", "x"),
                ("crates/a/src/own.rs", d.SPDX_DUAL, "元帥", "2026-11-26", "x"),
            ])

    def test_external_spdx_is_kept_as_written(self):
        out = self.decide([("crates/a/src/ported.rs", "Apache-2.0 AND Unlicense", "元帥", "2026-11-26", "M3")])
        self.assertEqual(out["crates/a/src/ported.rs"][0], "Apache-2.0 AND Unlicense")


class NewFiles(unittest.TestCase):
    """新規ファイルは、自作も含めて、まず Apache-2.0（元帥の規則、2026-11-26）。"""

    BATCH = [("@要確認", d.SPDX_APACHE, "元帥", "2026-11-26", "決定 2")]

    def decide(self, rows):
        decisions = [(n + 2, *dec) for n, dec in enumerate(self.BATCH)]
        return d.apply_decisions(rows, decisions, "d.tsv")

    def test_a_crates_file_missing_from_the_previous_list_is_marked(self):
        rows = [row("crates/a/src/new_own.rs")]
        marked = c.mark_new_files(rows, {})
        self.assertEqual(marked[0][5], "要確認")
        self.assertEqual(marked[0][6], c.NEW_FILE_REASON)
        self.assertEqual(self.decide(marked)["crates/a/src/new_own.rs"][0], d.SPDX_APACHE)

    def test_an_existing_held_file_stays_held(self):
        rows = [row("crates/a/src/old_own.rs")]
        previous = {"crates/a/src/old_own.rs": ("", "")}
        marked = c.mark_new_files(rows, previous)
        self.assertEqual(marked[0][5], "")
        self.assertEqual(self.decide(marked)["crates/a/src/old_own.rs"][0], d.HELD)

    def test_the_mark_is_sticky_across_regenerations(self):
        rows = [row("crates/a/src/new_own.rs")]
        previous = {"crates/a/src/new_own.rs": ("要確認", c.NEW_FILE_REASON)}
        marked = c.mark_new_files(rows, previous)
        self.assertEqual(marked[0][5], "要確認")
        self.assertEqual(marked[0][6], c.NEW_FILE_REASON)

    def test_other_scope_and_already_flagged_rows_are_untouched(self):
        rows = [
            row("docs/new.md", verdict="Pekko 由来", kind="Pekko言及", scope="other"),
            row("crates/a/src/flagged.rs", verdict="Pekko 由来", kind="原典同名", flag="要確認"),
        ]
        marked = c.mark_new_files(rows, {})
        self.assertEqual(marked[0][5], "")
        self.assertEqual(marked[1][6], "")


class DecisionFile(unittest.TestCase):
    """決定のファイルの検査（設計レビュー 3 回目の M2、L1 (c)）。"""

    HEADER = "\t".join(d.DECISION_HEADER) + "\n"

    def load(self, *lines):
        import tempfile

        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "decisions.tsv"
            path.write_text(self.HEADER + "".join(line + "\n" for line in lines), encoding="utf-8")
            return d.load_decisions(path)

    def test_accepts_the_three_forms_only(self):
        for spdx in (d.SPDX_APACHE, d.SPDX_DUAL, "Apache-2.0 AND Unlicense", "Apache-2.0 AND BSD-2-Clause"):
            self.assertEqual(self.load(f"crates/a.rs\t{spdx}\t元帥\t2026-11-26\tx")[0][2], spdx)
        for spdx in (
            "Apache-2.0 OR MIT",
            "MIT",
            "MIT  OR Apache-2.0",
            "mit or apache-2.0",
            "(MIT OR Apache-2.0)",
            "GPL-3.0-only",
            "Apache-2.0 AND MIT",
            "Apache-2.0 AND GPL-2.0-only",
        ):
            with self.assertRaises(SystemExit, msg=spdx) as raised:
                self.load(f"crates/a.rs\t{spdx}\t元帥\t2026-11-26\tx")
            self.assertIn(":2:", str(raised.exception), "行番号を出す")

    def test_date_must_be_a_real_iso_date(self):
        for date in ("YYYY-MM-DD", "2026-11-31", "2026/11/26", "26-11-26"):
            with self.assertRaises(SystemExit, msg=date):
                self.load(f"crates/a.rs\t{d.SPDX_APACHE}\t元帥\t{date}\tx")

    def test_a_proposal_cannot_become_a_decision_by_dropping_the_hash(self):
        proposal = f"crates/a.rs\tApache-2.0 AND Unlicense\t\t\t提案"
        self.assertEqual(self.load("# " + proposal), [])
        with self.assertRaises(SystemExit):
            self.load(proposal)
        with self.assertRaises(SystemExit):
            self.load(f"# crates/a.rs\tApache-2.0 AND Unlicense\t元帥\tYYYY-MM-DD\t提案")

    def test_the_committed_file_loads(self):
        root = Path(__file__).resolve().parents[2]
        rows = d.load_decisions(root / d.DECISIONS_DEFAULT)
        self.assertEqual([(target, spdx) for _n, target, spdx, *_ in rows], [(d.DECISION_FLAGGED, d.SPDX_APACHE)])


def manifest_row(number, path, spdx, source, verdict="Pekko 由来", kind="Mirrors", flag="", scope="crates"):
    return (number, {
        "path": path, "判定": verdict, "根拠の種類": kind, "根拠の中身": "", "要確認": flag,
        "要確認の理由": "", "走査範囲": scope, "付けるSPDX": spdx, "決定の出典": source,
    })


class ManifestCheck(unittest.TestCase):
    """`--check --manifest` の原典の要らない部分。設計レビュー 3 回目の試作 M-b、M-c、M-f、M-j、M-k が落ちる。"""

    BATCH = (2, "@要確認", d.SPDX_APACHE, "元帥", "2026-11-26", "決定 2")
    BATCH_SOURCE = "d.tsv:2（元帥 2026-11-26）"
    ASK = "crates/a/src/adaptor/ask_driver.rs"
    PORTED = "crates/a/tests/frequency_sketch_spec.rs"
    OWN = "crates/a/src/own.rs"

    def manifest(self):
        return [
            manifest_row(2, self.ASK, d.SPDX_APACHE, self.BATCH_SOURCE, kind="原典同名", flag="要確認"),
            manifest_row(3, self.PORTED, d.SPDX_APACHE, "判定", kind="移植マーカー"),
            manifest_row(4, self.OWN, d.HELD, "", verdict="自分で書いた", kind="なし"),
        ]

    def files(self, **spdx):
        files = {self.ASK: d.SPDX_APACHE, self.PORTED: d.SPDX_APACHE, self.OWN: None}
        files.update({key.replace("__", "/"): value for key, value in spdx.items()})
        return files

    def errors(self, manifest, files, decisions=(BATCH,)):
        return d.manifest_errors(manifest, list(decisions), "d.tsv", files)

    def kinds(self, errors):
        return {error.split()[0] for error in errors}

    def test_the_stage_a_state_passes(self):
        self.assertEqual(self.errors(self.manifest(), self.files()), [])

    def test_m_b_hand_edited_spdx_cell(self):
        manifest = self.manifest()
        manifest[0][1]["付けるSPDX"] = d.SPDX_DUAL
        files = self.files()
        files[self.ASK] = d.SPDX_DUAL
        self.assertTrue({"manifest-tampered", "mit-without-decision"} <= self.kinds(self.errors(manifest, files)))

    def test_m_c_row_removed_from_the_manifest(self):
        files = self.files()
        files[self.ASK] = d.SPDX_DUAL
        self.assertIn("mit-without-decision", self.kinds(self.errors(self.manifest()[1:], files)))

    def test_m_f_pekko_file_copied_to_a_new_path(self):
        files = self.files()
        files["crates/a/src/adaptor/ask_driver_copy.rs"] = d.SPDX_DUAL
        self.assertIn("mit-without-decision", self.kinds(self.errors(self.manifest(), files)))

    def test_m_j_moved_and_old_row_removed(self):
        files = self.files()
        del files[self.ASK]
        files["crates/a/src/adaptor/moved/ask_driver.rs"] = d.SPDX_DUAL
        self.assertIn("mit-without-decision", self.kinds(self.errors(self.manifest()[1:], files)))
        self.assertIn("manifest-missing-file", self.kinds(self.errors(self.manifest(), files)))

    def test_m_k_misspelled_mit_on_a_ported_file(self):
        files = self.files()
        files[self.PORTED] = "Apache-2.0 OR MIT"
        self.assertTrue({"spdx-grammar", "mit-without-decision"} <= self.kinds(self.errors(self.manifest(), files)))
        with self.assertRaises(SystemExit):
            DecisionFile().load(f"{self.PORTED}\tApache-2.0 OR MIT\t元帥\t2026-11-26\tx")

    def test_new_files_take_apache_only(self):
        files = self.files()
        files["crates/a/src/new.rs"] = "Apache-2.0 AND Unlicense"
        self.assertIn("new-file-not-apache", self.kinds(self.errors(self.manifest(), files)))

    def test_a_per_file_decision_allows_mit_and_the_recomputed_cells(self):
        decision = (3, self.OWN, d.SPDX_DUAL, "元帥", "2026-12-01", "照合済み")
        manifest = self.manifest()
        manifest[2][1].update({"付けるSPDX": d.SPDX_DUAL, "決定の出典": "d.tsv:3（元帥 2026-12-01）"})
        files = self.files()
        files[self.OWN] = d.SPDX_DUAL
        self.assertEqual(self.errors(manifest, files, (self.BATCH, decision)), [])

    def test_mit_on_strong_evidence_stops_whatever_the_spelling(self):
        with self.assertRaises(SystemExit):
            d.apply_decisions(
                [(self.PORTED, "crates", "Pekko 由来", "移植マーカー", "", "", "")],
                [(2, self.PORTED, "Apache-2.0 OR MIT", "元帥", "2026-11-26", "x")],
                "d.tsv",
            )

    def test_the_committed_manifest_recomputes_from_the_decisions(self):
        root = Path(__file__).resolve().parents[2]
        manifest = d.read_manifest(root / "docs" / "design" / "issue-206-file-origin.tsv")
        decisions = d.load_decisions(root / d.DECISIONS_DEFAULT)
        files = {cells["path"]: None for _n, cells in manifest}
        self.assertEqual(d.manifest_errors(manifest, decisions, d.DECISIONS_DEFAULT.name, files), [])


class LeadingBlocks(unittest.TestCase):
    def test_keeps_every_block_before_package_verbatim(self):
        source = (
            "/*\n * Licensed to the Apache Software Foundation (ASF) under one or more\n"
            " *\n *   https://www.apache.org/licenses/LICENSE-2.0\n */\n\n"
            "/*\n * Copyright 2017 The Netty Project\n */\n\n"
            "//The 'Requiring' class is derived from scala-library code.\n"
            "package org.apache.pekko.io.dns\n/* not header */\n"
        )
        self.assertEqual(
            p.leading_blocks(source),
            [
                "Licensed to the Apache Software Foundation (ASF) under one or more",
                "",
                "  https://www.apache.org/licenses/LICENSE-2.0",
                "",
                "Copyright 2017 The Netty Project",
                "",
                "//The 'Requiring' class is derived from scala-library code.",
            ],
        )

    def test_third_party_owner_lines_skip_asf_and_lightbend_only(self):
        lines = [
            "Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>",
            "Copyright EPFL and Lightbend, Inc.",
        ]
        owners = [line for line in lines if p.COPYRIGHT.search(line) and not p.OWN_NOTICE.search(line)]
        self.assertEqual(owners, ["Copyright EPFL and Lightbend, Inc."])

    def test_third_party_by_source_by_name_prefix_and_by_module_root(self):
        dns = "actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala"
        notices = {dns: ["Copyright 2017 The Netty Project"]}
        label = "Copyright 2017 The Netty Project（DnsSettings.scala）"
        self.assertEqual(p.third_party_evidence("fn f() {}\n", [dns], notices), [label])
        self.assertEqual(p.third_party_evidence("use super::DnsSettingsValues;\n", [], notices), [label])
        self.assertEqual(p.third_party_evidence("fn f() {}\n", [], notices), [])
        import classify_file_origin as cli

        self.assertEqual(cli.inherit_notices([], [label, "hash-prospector: public domain（Unlicense）"]), [label])


class Robustness(unittest.TestCase):
    """PR #215 のレビュー（CodeRabbit）の指摘の回帰テスト。"""

    def test_a_bare_package_name_still_yields_evidence(self):
        text = "// See org.apache.pekko\nfn f() {}\n"
        self.assertTrue(e.pekko_mentioned(text))
        self.assertEqual(e.mention_evidence(text, ".rs"), "// See org.apache.pekko")
        verdict = e.classify("docs/x.md", text, "other", {}, PEKKO)
        self.assertEqual(verdict[2], "// See org.apache.pekko")

    def test_a_reused_tree_must_carry_the_pinned_commit(self):
        import tempfile

        with tempfile.TemporaryDirectory() as raw:
            tree = Path(raw)
            (tree / "build.sbt").write_text("// another revision\n", encoding="utf-8")
            with self.assertRaises(SystemExit):
                p.pekko_tree(Path("/nonexistent"), tree)
            self.assertTrue((tree / "build.sbt").exists(), "利用者の展開先を消さない")
            (tree / p.TREE_STAMP).write_text(p.PEKKO_COMMIT + "\n", encoding="utf-8")
            self.assertEqual(p.pekko_tree(Path("/nonexistent"), tree), (tree, False))

    def test_a_temporary_tree_is_removed_when_extraction_fails(self):
        import glob
        import tempfile

        before = set(glob.glob(str(Path(tempfile.gettempdir()) / "pekko-v2.0.0-*")))
        with self.assertRaises(Exception):
            p.pekko_tree(Path("/nonexistent-pekko-repo"))
        after = set(glob.glob(str(Path(tempfile.gettempdir()) / "pekko-v2.0.0-*")))
        self.assertEqual(after - before, set())

    def test_moved_ids_are_masked_in_evidence(self):
        import classify_file_origin as cli

        old = "actor-testkit-typed/src/test/java/org/apache/pekko/X/JUnit5Test.java"
        self.assertEqual(cli.mask_moved(f"move {old} new/path", [old]), f"move {cli.MOVED_ID_MASK} new/path")

    def test_strict_marker_and_structured_header_stripping(self):
        import header_core as core

        self.assertIsNone(e.marker_evidence("// Imported from Pekko\n"))
        self.assertIsNone(e.marker_evidence("// ported from pekko\n"))
        self.assertIsNotNone(e.marker_evidence("// Ported from Pekko v2.0.0\n"))
        body = "//! plain module\nfn f() {}\n"
        headed = core.apply_header(body, "pekko")
        self.assertEqual(e.strip_own_header(headed), body)
        self.assertEqual(e.classify("crates/a/src/own.rs", e.strip_own_header(headed), "crates", {}, PEKKO)[0], "自分で書いた")


if __name__ == "__main__":
    unittest.main()
