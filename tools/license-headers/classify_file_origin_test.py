"""#206 の線引き（classify_file_origin.py と origin_*.py）の H1 の規則、人の決定の重ね方、原典の表示の切り出しのテスト。

原典の git リポジトリは要らない（型の索引は小さな辞書で与える）。
    python3 tools/license-headers/classify_file_origin_test.py
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

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


if __name__ == "__main__":
    unittest.main()
