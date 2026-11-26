#!/usr/bin/env python3
"""#206: crates/ ほかのファイルを「Pekko 由来」「自分で書いた」に機械判定し、TSV と集計を出す。

原典は Pekko v2.0.0（commit 9fbd2283b1cf701c90a1223bf621942c191db61f）。判定材料は次の 4 つ。

  1. 冒頭 20 行の移植マーカー。`tools/license-headers/apply_ported_headers.py` と同じ定義
     （Ported from Pekko / Port of Pekko / Checked against Pekko、大小文字を問わない、
     同スクリプトが付けるヘッダーを除いた本文の冒頭 20 行）。
  2. 原典への対応の記述（rustdoc/コメントの `Mirrors \\`org.apache.pekko.X\\``、原典パス
     `org/apache/pekko/...`、`Ported from` / `Port of`）。
  3. 台帳（ledger/nodes.yaml と revoked 含む retired）の rust.test_file。
  4. 原典に同名のファイル・クラスがあるか（参考。ファイル内に記述が無ければ要確認）。
  5. 上記が無く Pekko への言及だけがあるもの（設計の参考。要確認）。

方針（オーナー決定 = C 案の材料）：判定は 2 値。「迷うもの」は Pekko 由来へ倒し、
要確認の印と理由を付ける。区分「判断できない」は使わない。

使い方（リポジトリのルートで実行する。引数はすべて省略できる）:
    uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py

    --repo        既定はこのスクリプトから 2 つ上（リポジトリのルート）
    --pekko       原典の git リポジトリ。既定は環境変数 PEKKO_REPO、無ければ <repo>/pekko
                  （tools/pekko-source/prepare_pekko_source.sh が用意する）
    --out-tsv     既定は docs/design/issue-206-file-origin.tsv
    --out-summary 指定したときだけ集計の JSON を書く

出力の TSV（docs/design/issue-206-file-origin.tsv）は、このスクリプトが出した
結果そのものである（PR #215 の作業ツリーで流し直し、バイト単位で一致することを
確かめた）。ライセンス作業のためのファイル（SELF_EXCLUDE_FILES、
SELF_EXCLUDE_DIRS）は走査しないので、TSV を書き直しても自分自身は増えない。
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

PEKKO_COMMIT = "9fbd2283b1cf701c90a1223bf621942c191db61f"
MARKER = re.compile(r"(Ported from Pekko|Port of Pekko|Checked against Pekko)", re.IGNORECASE)
MARKER_WINDOW = 20
HEADER_FIRST = "// SPDX-License-Identifier: Apache-2.0"
HEADER_LAST = "// This file has been modified from the Apache Pekko original."
NEWLINES = ("\r\n", "\n")

# 2. 原典への対応（強い根拠）。上から順に採用する。
PORTED = re.compile(r"\b(Ported from|Port of)\b")
MIRRORS = re.compile(r"Mirrors[^\n]{0,160}")
PEKKO_PATH = re.compile(r"org[/.]apache[/.]pekko[/.][\w./]*")
# 5. Pekko への言及だけ（弱い根拠）。`pekko://` のような URI スキームや、
#    テストが使う名前（`PekkoCustomSpec` など）は原典への言及とみなさない。
#    Rust はコメント行だけを見る。
MENTION = re.compile(r"\bPekko\w*|apache/pekko")
PEKKO_PATH_ANY = re.compile(r"org[/.]apache[/.]pekko\b")
RUST_COMMENT = re.compile(r"^\s*(?://|/\*|\*)")
# 6. 原典を指す言及だが、それが Pekko だと書かれていないもの（弱い根拠）。
ORIGINAL = re.compile(r"\bthe original\b|\bupstream\b|\boriginal (?:impl|implementation|code|behaviour|behavior|spec|suite|class|test)s?\b", re.IGNORECASE)

SKIP_DIRS = {".git", "target", "node_modules", "pekko", ".takt", ".venv", "__pycache__"}
# ライセンス作業そのもののファイル。判定の対象ではなく、出力にも入れない
# （入れると、TSV を書き直すたびに自分自身が「Pekko 言及」として増える）。
SELF_EXCLUDE_FILES = {
    "LICENSE",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "NOTICE",
    "docs/design/issue-206-license-policy.md",
    "docs/design/issue-206-file-origin.tsv",
    ".spark-report.md",  # Orca のワーカーが作業ツリーに置く報告（追跡しない）
}
SELF_EXCLUDE_DIRS = ("tools/license-headers/",)
BINARY_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".pdf", ".jar", ".zip", ".gz", ".lock",
    ".woff", ".woff2", ".ttf", ".so", ".dylib", ".rlib", ".bin",
}
MAX_BYTES = 2 * 1024 * 1024
GENERIC_STEMS = {"mod", "lib", "main", "prelude", "tests", "test", "setup", "util", "utils", "index"}


def read_text(path):
    with path.open("r", encoding="utf-8", errors="replace", newline="") as handle:
        return handle.read()


def strip_own_header(text):
    """apply_ported_headers.py が付けたヘッダーを除く（冪等判定のため）。"""
    for newline in NEWLINES:
        header = newline.join(
            [
                HEADER_FIRST,
                "//",
                "// Derived from Apache Pekko, which was derived from Akka.",
                "// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>",
                "// Copyright 2022-2026 The Apache Software Foundation",
                "//",
                "// Licensed under the Apache License, Version 2.0. See LICENSE and NOTICE",
                "// at the repository root.",
                "//",
                HEADER_LAST,
            ]
        )
        prefix = header + newline + newline
        if text.startswith(prefix):
            return text[len(prefix):]
    return text


def marker_evidence(text):
    body = strip_own_header(text)
    head_lines = body.splitlines()[:MARKER_WINDOW]
    for line in head_lines:
        found = MARKER.search(line)
        if found:
            return found.group(1), line.strip()
    return None


def shorten(line):
    snippet = line.strip()
    return snippet if len(snippet) <= 160 else snippet[:157] + "..."


def first_line(text, pattern, require_pekko=False):
    for line in text.splitlines():
        if pattern.search(line) and (not require_pekko or MENTION.search(line)):
            return shorten(line)
    return None


def doc_evidence(text, pekko_any):
    """原典への対応の記述。最初に当たった 1 件を返す。

    `Ported from` / `Port of` は Pekko 以外（旧 Rust ツールなど）からの移植にも
    使われるので、その行が Pekko に触れているときだけ原典の対応とみなす。
    同じく `Mirrors` は、ファイルのどこかに Pekko の記述があるときだけ採用する。
    """
    ported = first_line(text, PORTED, require_pekko=True)
    if ported:
        return ("Ported from" if "Ported" in ported else "Port of"), ported
    if pekko_any:
        mirrors = first_line(text, MIRRORS)
        if mirrors:
            return "Mirrors", mirrors
        path = first_line(text, PEKKO_PATH)
        if path:
            return "原典パス", path
        return None
    mirrors = first_line(text, MIRRORS)
    if mirrors:
        return "Mirrors（原典の明記なし）", mirrors
    return None


def mention_evidence(text, suffix):
    lines = text.splitlines()
    if suffix == ".rs":
        # コメント行の言及を先に採る。コード行の `pekko://` などは最後の手段。
        for line in lines:
            if RUST_COMMENT.match(line) and MENTION.search(line):
                return shorten(line)
    for line in lines:
        if MENTION.search(line):
            return shorten(line)
    return None


def original_evidence(text, suffix):
    for line in text.splitlines():
        if suffix == ".rs" and not RUST_COMMENT.match(line):
            continue
        if ORIGINAL.search(line):
            return shorten(line)
    return None


def pekko_mentioned(text):
    return bool(MENTION.search(text) or PEKKO_PATH_ANY.search(text))


# ---- 原典の索引 ----------------------------------------------------------

def pekko_paths(git_repo):
    out = subprocess.run(
        ["git", "-C", str(git_repo), "ls-tree", "-r", "--name-only", PEKKO_COMMIT],
        check=True, capture_output=True, text=True,
    )
    return [line for line in out.stdout.splitlines() if line]


def pekko_tree(git_repo, target=None):
    """原典の tree を展開して、そのパスを返す（読むだけ。フェッチはしない）。

    戻り値は (展開先, 一時ディレクトリを作ったか)。呼び出し側が後始末に使う。
    """
    temporary = target is None
    if target is None:
        target = Path(tempfile.mkdtemp(prefix="pekko-v2.0.0-"))
    elif (target / "build.sbt").exists():
        return target, False
    else:
        target.mkdir(parents=True, exist_ok=True)
    archive = subprocess.run(
        ["git", "-C", str(git_repo), "archive", PEKKO_COMMIT], check=True, capture_output=True
    )
    subprocess.run(["tar", "-x", "-C", str(target)], input=archive.stdout, check=True)
    return target, temporary


def pekko_index(paths):
    """原典のファイル名（拡張子抜き、小文字）から原典パスへの索引。"""
    index = defaultdict(list)
    for path in paths:
        if not path.endswith((".scala", ".java")):
            continue
        stem = Path(path).stem.lower()
        index[stem].append(path)
    return index


DECL = re.compile(
    r"^\s*(?:(?:final|sealed|abstract|case|implicit|private\[[\w.]+\]|protected\[[\w.]+\])\s+)*"
    r"(?:class|trait|object|interface|enum|record)\s+([A-Z][A-Za-z0-9_]*)",
    re.MULTILINE,
)
RUST_DECL = re.compile(
    r"^\s*pub(?:\s*\([^)]*\))?\s+(?:struct|enum|trait|union)\s+([A-Z][A-Za-z0-9_]*)",
    re.MULTILINE,
)


def pekko_types(tree):
    """原典の型名から原典パスへの索引（同じ名前の宣言が多いものは使わない）。"""
    index = defaultdict(list)
    for path in tree.rglob("*"):
        if path.suffix not in {".scala", ".java"} or not path.is_file():
            continue
        rel = str(path.relative_to(tree))
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            found = DECL.match(line)
            if found:
                index[found.group(1)].append(rel)
    return index


def type_evidence(text, types, index):
    """ファイル内で宣言している公開型のうち、原典に同名の型があるもの。"""
    for name in RUST_DECL.findall(text):
        if len(name) < 9:
            continue
        hits = types.get(name)
        if not hits or len(hits) > 6:
            continue
        # テスト専用ファイルにしか無い名前は、本体の型とみなさない。
        if not any("/main/" in path for path in hits):
            continue
        best = sorted(hits, key=lambda path: ("/main/" not in path, not path.endswith(".scala")))[0]
        return f"{name} <- {best}"
    return None


def camel(stem):
    return "".join(part.capitalize() for part in stem.split("_") if part)


def name_candidates(rust_path):
    stem = Path(rust_path).stem
    bases = [stem]
    for suffix in ("_spec", "_test", "_tests"):
        if stem.endswith(suffix):
            bases.append(stem[: -len(suffix)])
    candidates = set()
    for base in bases:
        if len(base) < 4 or base.lower() in GENERIC_STEMS:
            continue
        candidates.add(base.lower())
        candidates.add(base.lower().replace("_", ""))
        camel_name = camel(base)
        if camel_name:
            candidates.add(camel_name.lower())
    return {candidate for candidate in candidates if len(candidate) >= 4}


def name_evidence(rust_path, index):
    if Path(rust_path).suffix != ".rs":
        return None
    for candidate in sorted(name_candidates(rust_path)):
        hits = index.get(candidate)
        if hits:
            # 本体の定義（main 配下の Scala）を先に見る。docs のサンプルは後に回す。
            hits = sorted(hits, key=lambda path: ("/main/" not in path, not path.endswith(".scala")))
            return hits[0]
    return None


# ---- 台帳 ---------------------------------------------------------------

def ledger_index(repo):
    import yaml

    data = yaml.safe_load((repo / "ledger" / "nodes.yaml").read_text(encoding="utf-8"))
    index = {}
    for group in ("nodes", "retired"):
        for node in data.get(group) or []:
            test_file = (node.get("rust") or {}).get("test_file")
            if test_file:
                index[test_file] = (node.get("id"), group)
    return index


# ---- 走査 ---------------------------------------------------------------

def target_files(repo):
    """crates/** の *.rs は全件。tools/ scripts/ docs/ とトップの一部は、
    判定材料が 1 つでも当たったものだけを返す（可能性のあるもの）。"""
    crates = []
    others = []
    for path in sorted(repo.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(repo)
        parts = rel.parts
        if any(part in SKIP_DIRS for part in parts):
            continue
        posix = rel.as_posix()
        if posix in SELF_EXCLUDE_FILES or posix.startswith(SELF_EXCLUDE_DIRS):
            continue
        if path.suffix in BINARY_SUFFIXES or path.stat().st_size > MAX_BYTES:
            continue
        if parts[0] == "crates":
            if path.suffix == ".rs":
                crates.append(str(rel))
            continue
        others.append(str(rel))
    return crates, others


STRONG = {"移植マーカー", "Ported from", "Port of", "Mirrors", "原典パス"}
FLAG_BY_KIND = {
    "台帳": "台帳には移植先として記録されているが、ファイル内に原典対応の記述とマーカーが無い",
    "原典同名": "ファイル内に Pekko の記述が無く、原典に同名のファイルがあるだけ",
    "原典同名（型名）": "ファイル内に Pekko の記述が無いが、宣言している公開型と同名の型が原典にある",
    "Pekko言及": "Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認）",
    "原典への言及（Pekko の明記なし）": "「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い",
    "Mirrors（原典の明記なし）": "「original」への対応は書かれているが、それが Pekko だと明記が無い",
}


def classify(rel, text, ledger, index, scope, types):
    marker = marker_evidence(text)
    pekko_any = pekko_mentioned(text)
    doc = doc_evidence(text, pekko_any)
    ledger_hit = ledger.get(rel)
    name_hit = name_evidence(rel, index) if scope == "crates" else None
    type_hit = type_evidence(text, types, index) if scope == "crates" else None

    if marker:
        kind, snippet = "移植マーカー", marker[1]
    elif doc:
        kind, snippet = doc
    elif original_evidence(text, Path(rel).suffix):
        kind, snippet = "原典への言及（Pekko の明記なし）", original_evidence(text, Path(rel).suffix)
    elif ledger_hit:
        kind, snippet = "台帳", f"{ledger_hit[0]} ({ledger_hit[1]}=rust.test_file)"
    elif name_hit:
        kind, snippet = "原典同名", name_hit
    elif type_hit:
        kind, snippet = "原典同名（型名）", type_hit
    elif pekko_any:
        kind, snippet = "Pekko言及", mention_evidence(text, Path(rel).suffix)
    else:
        return ("自分で書いた", "なし", "", "", "")

    # 判定は Pekko 由来。要確認の印を決める。
    flag = ""
    reason = ""
    if scope != "crates":
        # 文書・ツールは Pekko のコードの転載かどうかを機械で決められない。
        flag = "要確認"
        if kind in STRONG or kind.startswith("Mirrors"):
            reason = "文書/ツール: 原典のパスや記述を引く（転載の有無は未確認）"
        else:
            reason = "文書/ツール: Pekko の語が出るだけ（転載の可能性は低い）"
    elif kind not in STRONG:
        flag, reason = "要確認", FLAG_BY_KIND[kind]
    elif not ledger_hit and rel.endswith(".rs") and "/tests/" in rel:
        flag, reason = "要確認", "移植の根拠はあるが台帳の test_file に無い"

    return ("Pekko 由来", kind, snippet, flag, reason)


KIND_ORDER = [
    "移植マーカー",
    "Ported from",
    "Port of",
    "Mirrors",
    "原典パス",
    "台帳",
    "Mirrors（原典の明記なし）",
    "原典への言及（Pekko の明記なし）",
    "原典同名",
    "原典同名（型名）",
    "Pekko言及",
]


def markdown(rows, summary, tsv_name, script_name):
    def count(seq):
        return Counter(seq)

    crates_rows = [row for row in rows if row[1] == "crates"]
    other_rows = [row for row in rows if row[1] == "other"]
    out = []
    add = out.append

    add("# #206: crates/ ほかのファイルの由来（Pekko 移植 / 自分で書いた）")
    add("")
    add("issue: https://github.com/j5ik2o/fraktor-ng-rs/issues/206  ")
    add("ライセンス方針の材料（オーナー決定 = C: 移植ファイルは Apache-2.0 だけ、"
        "自分で書いた部分は MIT OR Apache-2.0）。この文書は判定の一覧であって、方針の決定ではない。")
    add("")
    add(f"原典は Apache Pekko v2.0.0（commit `{PEKKO_COMMIT}`）。")
    add("判定は機械で行い、材料が食い違うもの・弱いものは「Pekko 由来」に倒して「要確認」の印を付けた。")
    add("区分「判断できない」は使っていない。")
    add("")
    add("## 1. 件数の内訳")
    add("")
    add("| 走査範囲 | 件数 | Pekko 由来 | 自分で書いた | うち要確認 |")
    add("|:---|--:|--:|--:|--:|")
    for scope, label, subset in (
        ("crates", "crates/**/*.rs（全件）", crates_rows),
        ("other", "crates/ 以外（docs/ tools/ ledger/ scripts/ とトップ。材料が当たったもの）", other_rows),
    ):
        verdicts = count(row[2] for row in subset)
        add(f"| {label} | {len(subset)} | {verdicts.get('Pekko 由来', 0)} | "
            f"{verdicts.get('自分で書いた', 0)} | {sum(1 for row in subset if row[5])} |")
    verdicts = count(row[2] for row in rows)
    add(f"| **合計** | **{len(rows)}** | **{verdicts.get('Pekko 由来', 0)}** | "
        f"**{verdicts.get('自分で書いた', 0)}** | **{sum(1 for row in rows if row[5])}** |")
    add("")
    add("### 根拠の種類べつ")
    add("")
    add("| 根拠の種類 | crates | docs/tools ほか | 合計 | 要確認 |")
    add("|:---|--:|--:|--:|--:|")
    kinds = count(row[3] for row in rows)
    for kind in KIND_ORDER + [k for k in kinds if k not in KIND_ORDER]:
        if not kinds.get(kind):
            continue
        in_crates = sum(1 for row in crates_rows if row[3] == kind)
        in_other = sum(1 for row in other_rows if row[3] == kind)
        flagged = sum(1 for row in rows if row[3] == kind and row[5])
        add(f"| {kind} | {in_crates} | {in_other} | {kinds[kind]} | {flagged} |")
    add("")
    add("### 要確認の理由べつ")
    add("")
    add("| 要確認の理由 | 件数 |")
    add("|:---|--:|")
    for reason, number in count(row[6] for row in rows if row[6]).most_common():
        add(f"| {reason} | {number} |")
    add("")
    add("### crates のクレートべつ")
    add("")
    add("| クレート | Pekko 由来 | 自分で書いた | うち要確認 |")
    add("|:---|--:|--:|--:|")
    for crate, _ in count("/".join(row[0].split("/")[:2]) for row in crates_rows).most_common():
        subset = [row for row in crates_rows if row[0].startswith(crate + "/")]
        verdicts = count(row[2] for row in subset)
        add(f"| {crate} | {verdicts.get('Pekko 由来', 0)} | {verdicts.get('自分で書いた', 0)} | "
            f"{sum(1 for row in subset if row[5])} |")
    add("")

    add("## 2. 判定の規則")
    add("")
    add("材料は次の 8 つ。ファイルごとに、上から順に当たった 1 つを根拠として記録する。")
    add("")
    add("1. **移植マーカー**：冒頭 20 行の `Ported from Pekko` / `Port of Pekko` / `Checked against Pekko`"
        "（大小文字を問わない）。定義は `tools/license-headers/apply_ported_headers.py` と同じで、"
        "同スクリプトが付けるヘッダーを除いた本文の先頭 20 行を見る。")
    add("2. **Ported from / Port of**：出現行が Pekko に触れているものだけを原典の対応とみなす"
        "（`Ported from the old ambiguous-suffix-lint` のような、旧 Rust ツールからの移植を"
        "拾わないため）。")
    add("3. **Mirrors**：`Mirrors org.apache.pekko.X` などの対応の記述。ファイルのどこかに Pekko の"
        "記述があるときだけ採用する。")
    add("4. **原典パス**：`org.apache.pekko.X` / `org/apache/pekko/...` の形のパス。")
    add("5. **台帳**：`ledger/nodes.yaml` の `rust.test_file`（`retired` も含む）。")
    add("6. **原典同名 / 原典同名（型名）**：ファイル名（拡張子抜き）か、ファイルが宣言している公開型の名前が、"
        "原典のファイル名・型名と一致する。")
    add("7. **Pekko言及**：コメント行（`.rs`）または本文に `Pekko` の語があるだけ。")
    add("8. **原典への言及（Pekko の明記なし）**：`the original` / `upstream` への言及があるが、"
        "それが Pekko だと書かれていない。")
    add("")
    add("判定は 2 値。")
    add("")
    add("- **Pekko 由来**：材料 1〜5 のいずれかが当たったもの。材料 6〜8 しか当たらないものも、"
        "迷うものとして Pekko 由来に倒した。")
    add("- **自分で書いた**：どの材料も当たらなかったもの（crates/**/*.rs の全件を判定）。")
    add("")
    add("**要確認**は、Pekko 由来のうち次のいずれか。")
    add("")
    for kind in KIND_ORDER:
        if kind in STRONG or not FLAG_BY_KIND.get(kind):
            continue
        add(f"- {FLAG_BY_KIND[kind]}")
    add("- 移植の根拠はあるが台帳の `test_file` に無い（統合テストのみ）。")
    add("- 文書・ツールは、Pekko のコードの転載かどうかを機械で決められないため、材料が当たった時点で要確認。"
        "原典のパスや記述を引くものと、Pekko の語が出るだけのものに分けてある。")
    add("")
    add("## 3. 台帳との突合")
    add("")
    add(f"- 台帳の `rust.test_file` は {summary['ledger_test_files']} 件（`nodes` 51 + `retired` 1）。"
        "すべてこの一覧に入っている。")
    add("- 移植マーカーのあるファイル（51 件）は、"
        "`docs/design/issue-206-license-policy.md` §1.2 の実測 51 件と一致する。")
    add("- 台帳だけが持つ `crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs` はマーカーを持たないが、"
        "`Mirrors org.apache.pekko.testkit.PekkoSpec` の記述で Pekko 由来と判定した。")
    add("- 台帳に無いのに Pekko の原典パスを書いている統合テストが 2 件ある（要確認）。")
    add("")
    add("## 4. 要確認の一覧（元帥が扱いを決める）")
    add("")
    add("理由ごとに分けた。同じ理由のものは、同じ扱いでまとめて決められるはずである。")
    add("")
    by_reason = defaultdict(list)
    for row in rows:
        if row[5]:
            by_reason[row[6]].append(row)
    for reason, group in sorted(by_reason.items(), key=lambda item: -len(item[1])):
        add(f"### {reason} — {len(group)} 件")
        add("")
        for row in sorted(group):
            add(f"- `{row[0]}` — {row[3]}: {row[4].replace(chr(9), ' ')}")
        add("")

    add("## 5. 既知の限界・誤判定")
    add("")
    add("- 判定はファイル単位で、原典のどの部分を写したかまでは見ていない。"
        "Pekko 由来のファイルでも、Rust 向けに書き直した部分と原典のコードが残る部分が混ざる。"
        "どこまでが移植かはファイルごとに人が確かめる必要がある"
        "（`docs/design/issue-206-license-policy.md` §2 の二段階目）。")
    add("- `Ported from` は Pekko 以外からの移植にも使われる。`tools/syn-check/src/rules/*.rs` の "
        "`Ported from the old *-lint` は旧 Rust ツールからの移植で、Pekko とは関係ない。"
        "そのため出現行が Pekko に触れていることを条件にした。")
    add("- `pekko://test/user/x` のような URI スキームや、テストが使う名前（`PekkoCustomSpec`）は"
        "原典への言及とみなさない。`.rs` ではコメント行の `Pekko` だけを材料 7 に使う。")
    add("- 原典同名（材料 6）は弱い。原典のクラスが別のファイルに定義されている場合（例：`RootActorPath` は "
        "`ActorPath.scala`）は型名の照合で拾うが、名称が一致しない移植は拾えない。")
    add("- 文書（`docs/`）は Pekko のパスやクラス名を引用するだけのものが多い。"
        "それらも「Pekko 由来・要確認」に入れてある。転載が無ければ「自分で書いた」に落としてよい。")
    add("- この一覧は機械判定である。TSV の 1 行ずつが根拠を持ち、人がその根拠を見て覆せる形にしてある。")
    add("")
    add("## 6. 出力と再実行")
    add("")
    add(f"- この一覧（TSV、7 列）: `{tsv_name}`")
    add(f"- 判定スクリプト: `{script_name}`")
    add("")
    add("```sh")
    add(f"python3 {script_name} \\")
    add("  --repo <fraktor-ng-rs の作業ツリー> \\")
    add("  --pekko <Pekko v2.0.0 の git リポジトリ> \\")
    add(f"  --out-tsv {tsv_name} --out-md <この md> --out-summary /tmp/issue-206-summary.json")
    add("```")
    add("")
    add("列は `path`、`判定`、`根拠の種類`、`根拠の中身`、`要確認`、`要確認の理由`、`走査範囲`。")
    add("`path` はリポジトリのルートからの相対パス。")
    add("")
    return "\n".join(out) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    repo_default = Path(__file__).resolve().parents[2]
    parser.add_argument("--repo", type=Path, default=repo_default, help="リポジトリのルート（既定はこのスクリプトから 2 つ上）")
    parser.add_argument(
        "--pekko",
        type=Path,
        default=Path(os.environ.get("PEKKO_REPO") or repo_default / "pekko"),
        help="原典の git リポジトリ（既定は PEKKO_REPO、無ければ <repo>/pekko）",
    )
    parser.add_argument(
        "--out-tsv",
        type=Path,
        default=repo_default / "docs" / "design" / "issue-206-file-origin.tsv",
        help="一覧の TSV（既定は docs/design/issue-206-file-origin.tsv）",
    )
    parser.add_argument("--out-summary", type=Path, default=None, help="集計の JSON（指定したときだけ書く）")
    parser.add_argument("--out-md", type=Path, default=None, help="レポートの md（省略時は書かない）")
    parser.add_argument("--pekko-tree", type=Path, default=None, help="原典の展開先（省略時は一時ディレクトリ）")
    args = parser.parse_args(argv)

    repo = args.repo.resolve()
    index = pekko_index(pekko_paths(args.pekko.resolve()))
    tree, temporary_tree = pekko_tree(args.pekko.resolve(), args.pekko_tree)
    types = pekko_types(tree)
    ledger = ledger_index(repo)
    crates, others = target_files(repo)

    rows = []
    for rel in crates:
        text = read_text(repo / rel)
        rows.append((rel, "crates") + classify(rel, text, ledger, index, "crates", types))
    for rel in others:
        text = read_text(repo / rel)
        verdict = classify(rel, text, ledger, index, "other", types)
        if verdict[1] == "なし":
            continue
        rows.append((rel, "other") + verdict)

    header = [
        "path",
        "判定",
        "根拠の種類",
        "根拠の中身",
        "要確認",
        "要確認の理由",
        "走査範囲",
    ]
    lines = ["\t".join(header)]
    for rel, scope, verdict, kind, snippet, flag, reason in rows:
        lines.append("\t".join([rel, verdict, kind, snippet.replace("\t", " "), flag, reason, scope]))
    args.out_tsv.write_text("\n".join(lines) + "\n", encoding="utf-8")

    summary = {
        "rows": len(rows),
        "crates_rs": len(crates),
        "others_scanned": len(others),
        "others_listed": sum(1 for row in rows if row[1] == "other"),
        "verdict": dict(Counter(row[2] for row in rows)),
        "verdict_crates": dict(Counter(row[2] for row in rows if row[1] == "crates")),
        "verdict_others": dict(Counter(row[2] for row in rows if row[1] == "other")),
        "kind": dict(Counter(row[3] for row in rows)),
        "flag": dict(Counter(row[5] or "(なし)" for row in rows)),
        "flag_by_kind": dict(Counter(row[3] for row in rows if row[5])),
        "reason": dict(Counter(row[6] for row in rows if row[6])),
        "by_top_dir": dict(Counter("/".join(row[0].split("/")[:2]) for row in rows if row[1] == "crates")),
        "others_by_top_dir": dict(Counter(row[0].split("/")[0] for row in rows if row[1] == "other")),
        "ledger_test_files": len(ledger),
        "ledger_test_files_flagged": sorted(rel for rel in ledger if any(r[0] == rel and r[5] for r in rows)),
        "ledger_test_files_not_listed": sorted(rel for rel in ledger if not any(r[0] == rel for r in rows)),
    }
    if args.out_summary:
        args.out_summary.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    if args.out_md:
        args.out_md.write_text(
            markdown(rows, summary, args.out_tsv.name, Path(__file__).name), encoding="utf-8"
        )
    if temporary_tree:
        shutil.rmtree(tree, ignore_errors=True)
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
