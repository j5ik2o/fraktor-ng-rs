"""#206 ヘッダー純粋関数の単体テスト（F1〜F8）と seed 固定 PBT（P1〜P6、5,000 ケース以上）。"""
import random
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import header_check as hc  # noqa: E402
import header_core as core  # noqa: E402
import notice_render as nr  # noqa: E402

PBT_CASES = 5000
PBT_SEED = 20260206


def _gen_random_rust(rng):
    bom = "\ufeff" if rng.random() < 0.12 else ""
    shebang = ""
    if rng.random() < 0.1:
        shebang = rng.choice(["#!/usr/bin/env rust-script\n", "#!/bin/sh\r\n", "#! custom\n"])
    nl = "\r\n" if rng.random() < 0.25 else "\n"
    line_count = rng.choices([0, 1, 2, 5, 22], weights=[15, 20, 25, 25, 15])[0]
    pool = [
        "",
        "fn main() {}",
        "pub struct Foo(u32);",
        "//! inner doc",
        "/// outer doc",
        "#![allow(dead_code)]",
        "#!  [cfg(test)]",
        "// 日本語コメント\u2028行内セパレータ\u2029と\u0085",
        "// Imported from Pekko (false positive)",
        "// Support of Pekko (false positive)",
        "// UNSUPPORTED FROM PEKKO",
        "// Ported from Pekko v2.0.0",
        "/// Mirrors `org.apache.pekko.actor.Actor`.",
        "// SPDX-License-Identifier: Apache-2.0",
        "// This file has been modified from the Apache Pekko original.",
    ]
    lines = [rng.choice(pool) for _ in range(line_count)]
    if not lines:
        return bom + shebang
    sep_mixed = rng.random() < 0.1
    parts = []
    for idx, content in enumerate(lines):
        eol = ("\r\n" if idx % 2 == 0 else "\n") if sep_mixed else nl
        parts.append(content + eol)
    text = "".join(parts)
    if rng.random() < 0.3 and text.endswith(("\r\n", "\n")):
        text = text[:-2] if text.endswith("\r\n") else text[:-1]
    return bom + shebang + text


class HeaderCoreAndPbtTest(unittest.TestCase):
    def test_f1_structured_recognition_survives_year_copyright_old_text_and_e6(self):
        old_header = (
            "// SPDX-License-Identifier: Apache-2.0\n"
            "//\n"
            "// Derived from Apache Pekko, which was derived from Akka.\n"
            "// Copyright (C) 2018-2027 Lightbend Inc. <https://www.lightbend.com>\n"
            "// Copyright 2022-2027 The Apache Software Foundation\n"
            "//\n"
            "// Licensed under the Apache License, Version 2.0. See LICENSE and NOTICE\n"
            "// at the repository root.\n"
            "//\n"
            "// This file has been modified from the Apache Pekko original.\n"
        )
        # E6: 区切り空行が欠けて直後に // Ported from Pekko が接している。
        body = "// Ported from Pekko v2.0.0\nfn f() {}\n"
        combined = old_header + body
        removed, action = core.remove_header(combined)
        self.assertEqual(action, "removed")
        self.assertEqual(removed, body, "出典コメントを 1 バイトも削らない")
        # 同じ種別への apply は二重に SPDX を付けない。
        self.assertEqual(core.apply_header(combined, "pekko"), combined)
        # 空行欠落は --check で検知する。
        errs = [code for code, _ in hc.check_file(combined)]
        self.assertIn("missing-separator-blank", errs)

    def test_f2_marker_is_case_sensitive_with_word_boundaries_and_20_line_window(self):
        for false_pos in (
            "// Imported from Pekko\n",
            "// Support of Pekko\n",
            "// UNSUPPORTED FROM PEKKO\n",
            "// ported from pekko\n",
            "// Checked against PekkoX\n",
        ):
            self.assertFalse(core.has_ported_marker(false_pos), false_pos)
            self.assertFalse(core.has_c3_pekko_marker(false_pos), false_pos)

        # 20 行目は窓内、21 行目は窓外（F8）。
        line20 = "// filler\n" * 19 + "// Ported from Pekko v2.0.0\n"
        line21 = "// filler\n" * 20 + "// Ported from Pekko v2.0.0\n"
        self.assertTrue(core.has_ported_marker(line20))
        self.assertFalse(core.has_ported_marker(line21))

        # Mirrors は 21 行目より後でも C3 が検知する（M1）。
        late_mirrors = "// filler\n" * 130 + "/// Mirrors `org.apache.pekko.pattern.ask`.\n"
        self.assertTrue(core.has_c3_pekko_marker(late_mirrors))

    def test_f7_bom_shebang_inner_attr_and_unicode_newlines(self):
        # BOM 付き: BOM の直後に挿入され、往復する。
        bom_src = "\ufeff//! doc\nfn main() {}\n"
        applied = core.apply_header(bom_src, "pekko")
        self.assertTrue(applied.startswith("\ufeff// SPDX-License-Identifier: Apache-2.0\n"))
        self.assertEqual(core.remove_header(applied), (bom_src, "removed"))

        # #![...] と #!  [...] は内側属性であり shebang ではない（L3）。
        for attr in ("#![allow(dead_code)]\nfn f() {}\n", "#!  [cfg(test)]\nfn f() {}\n"):
            out = core.apply_header(attr, "pekko")
            self.assertTrue(out.startswith("// SPDX-License-Identifier: Apache-2.0\n"))
            self.assertEqual(core.remove_header(out), (attr, "removed"))

        # 改行あり shebang は 2 行目から挿入。
        shebang_src = "#!/usr/bin/env rust-script\nfn f() {}\n"
        out = core.apply_header(shebang_src, "pekko")
        self.assertTrue(out.startswith("#!/usr/bin/env rust-script\n// SPDX-License-Identifier: Apache-2.0\n"))
        self.assertEqual(core.remove_header(out), (shebang_src, "removed"))

        # Unicode 改行（U+2028/U+2029/U+0085）で行分割しない。
        uni = "// a\u2028b\u2029c\u0085d\nfn f() {}\n"
        self.assertEqual(len(core.split_lines_strict(uni)), 2)
        self.assertEqual(core.remove_header(core.apply_header(uni, "pekko")), (uni, "removed"))

    def test_exact_byte_match_for_pekko_and_original_headers(self):
        body = "//! module doc\n\npub fn hi() {}\n"
        expected_pekko = (
            "// SPDX-License-Identifier: Apache-2.0\n"
            "//\n"
            "// Licensed to the Apache Software Foundation (ASF) under one or more\n"
            "// license agreements; and to You under the Apache License, version 2.0:\n"
            "//\n"
            "//   https://www.apache.org/licenses/LICENSE-2.0\n"
            "//\n"
            "// This file is part of the Apache Pekko project, which was derived from Akka.\n"
            "//\n"
            "// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>\n"
            "//\n"
            "// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE\n"
            "// at the repository root.\n"
            "//\n"
            "// This file has been modified from the Apache Pekko original.\n"
            "\n"
            + body
        )
        self.assertEqual(core.apply_header(body, "pekko"), expected_pekko)

        expected_orig = (
            "// SPDX-License-Identifier: MIT OR Apache-2.0\n"
            "//\n"
            "// Copyright (c) 2026 Junichi Kato\n"
            "//\n"
            "// Licensed under either of Apache License, Version 2.0 or MIT license\n"
            "// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root.\n"
            "\n"
            + body
        )
        self.assertEqual(core.apply_header(body, "original", year=2026), expected_orig)

    def test_rejects_unrecognized_duplicate_spdx_line_too_long_and_p2_shebang(self):
        # P2: 改行なし shebang 単独 1 行は apply も check も拒否。
        for bad_shebang in ("#!/usr/bin/env rust", "\ufeff#!/bin/sh", "#!foo"):
            with self.assertRaises(core.HeaderError) as raised:
                core.apply_header(bad_shebang, "pekko")
            self.assertEqual(raised.exception.code, "shebang-no-newline")
            self.assertEqual(hc.check_file(bad_shebang)[0][0], "shebang-no-newline")

        # 未知 SPDX ブロックは apply・remove・check すべて拒否。
        unrec = "// SPDX-License-Identifier: Apache-2.0\n// hand-written header\n\nfn f() {}\n"
        with self.assertRaises(core.HeaderError):
            core.apply_header(unrec, "pekko")
        with self.assertRaises(core.HeaderError):
            core.remove_header(unrec)
        self.assertIn("unrecognized-header", [c for c, _ in hc.check_file(unrec)])

        # 本文中の SPDX 再出現は apply が拒否（L3 / I3）。
        dup = "fn f() {}\n// SPDX-License-Identifier: MIT\n"
        with self.assertRaises(core.HeaderError) as raised:
            core.apply_header(dup, "pekko")
        self.assertEqual(raised.exception.code, "duplicate-spdx")

        # 100 文字超えのヘッダー行は適用を断る（§3.1）。
        too_long = ("// " + "x" * 98,)
        with self.assertRaises(core.HeaderError) as raised:
            core.apply_header("fn f() {}\n", "pekko", notice_lines=too_long)
        self.assertEqual(raised.exception.code, "line-too-long")

    def test_p1_p3_p4_p5_pbt_5000_cases(self):
        rng = random.Random(PBT_SEED)
        accepted = 0
        rejected = 0
        for idx in range(PBT_CASES):
            src = _gen_random_rust(rng)
            kind = "pekko" if idx % 2 == 0 else "original"
            parsed = core.parse_header(src)
            if parsed.state != "NONE":
                rejected += 1
                continue
            try:
                applied = core.apply_header(src, kind, year=2026)
            except core.HeaderError:
                rejected += 1
                continue
            accepted += 1
            # P1: バイト完全往復。
            restored, action = core.remove_header(applied)
            self.assertEqual(action, "removed")
            self.assertEqual(restored, src)
            # P3: 冪等性。
            self.assertEqual(core.apply_header(applied, kind, year=2026), applied)
            # P4: 種別衝突拒否。
            other_kind = "original" if kind == "pekko" else "pekko"
            with self.assertRaises(core.HeaderError):
                core.apply_header(applied, other_kind, year=2026)
            # P5: 適用後は check 合格、除去後は check 不合格。
            self.assertEqual(hc.check_file(applied), [])
            self.assertNotEqual(hc.check_file(restored), [])
            # I4: マーカー判定の位置不変性。
            self.assertEqual(
                core.has_ported_marker(core.parse_header(applied).body),
                core.has_ported_marker(core.parse_header(src).body),
            )
        # P1 が空振りしていないこと（大部分が受理され、拒否ケースも混ざる）。
        self.assertGreater(accepted, PBT_CASES // 2)
        self.assertGreater(rejected, 0)

    def test_p6_modified_header_non_destructive_removal_5000_cases(self):
        rng = random.Random(PBT_SEED + 1)
        for idx in range(PBT_CASES):
            body = rng.choice([
                "// Ported from Pekko v2.0.0\nfn a() {}\n",
                "//! inner doc\nfn b() {}\n",
                "/// outer doc\npub struct S;\n",
                "fn c() {}",
            ])
            nl = "\r\n" if idx % 3 == 0 else "\n"
            year = 2020 + (idx % 10)
            sep = nl if idx % 2 == 0 else ""
            if idx % 2 == 0:
                header = nl.join([
                    "// SPDX-License-Identifier: Apache-2.0",
                    "//",
                    f"// Copyright (C) 2009-{year} Lightbend Inc.",
                    "// Licensed under the Apache License, Version 2.0. See LICENSE and NOTICE",
                    core.PEKKO_ANCHOR,
                ]) + nl + sep + body
                kind = "pekko"
            else:
                header = nl.join([
                    "// SPDX-License-Identifier: MIT OR Apache-2.0",
                    "//",
                    f"// Copyright (c) {year} Junichi Kato",
                    core.ORIGINAL_ANCHOR,
                ]) + nl + sep + body
                kind = "original"
            removed, action = core.remove_header(header)
            self.assertEqual(action, "removed")
            self.assertEqual(removed, body)
            if kind == "pekko" or not core.has_c3_pekko_marker(body):
                self.assertEqual(core.apply_header(header, kind, year=2026), header)

    def test_notice_render_deduplicates_blocks_and_handles_third_party(self):
        dns_src = (
            "/*\n * Licensed to the Apache Software Foundation (ASF) under one or more\n"
            " * license agreements; and to You under the Apache License, version 2.0:\n *\n"
            " *   https://www.apache.org/licenses/LICENSE-2.0\n *\n"
            " * This file is part of the Apache Pekko project, which was derived from Akka.\n */\n\n"
            "/*\n * Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>\n */\n\n"
            "/*\n * Copyright 2017 The Netty Project\n */\npackage org.apache.pekko.io.dns\n"
        )
        spec_src = (
            "/*\n * Licensed to the Apache Software Foundation (ASF) under one or more\n"
            " * license agreements; and to You under the Apache License, version 2.0:\n *\n"
            " *   https://www.apache.org/licenses/LICENSE-2.0\n *\n"
            " * This file is part of the Apache Pekko project, which was derived from Akka.\n */\n\n"
            "/*\n * Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>\n */\n"
            "package org.apache.pekko.io.dns\n"
        )
        files = {"DnsSettings.scala": dns_src, "DnsSettingsSpec.scala": spec_src}
        netty_blocks = nr.blocks_for_owner(dns_src, "Copyright 2017 The Netty Project")
        lines = nr.render_pekko_notice_lines(
            ["DnsSettingsSpec.scala"],
            [
                "Copyright 2017 The Netty Project（DnsSettings.scala）",
                core.SCALA_LIBRARY_LABEL,
                core.HASH_PROSPECTOR_LABEL,
            ],
            files.__getitem__,
            lambda owner, name: netty_blocks,
        )
        joined = "\n".join(lines)
        self.assertIn("// Copyright 2017 The Netty Project", joined)
        self.assertIn("// Copyright EPFL and Lightbend, Inc.", joined)
        self.assertNotIn("hash-prospector", joined)
        # 同じブロックが重複しないこと。
        self.assertEqual(joined.count("Copyright 2017 The Netty Project"), 1)
