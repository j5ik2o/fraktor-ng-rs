#!/usr/bin/env python3
"""#206: crates/ ほかのファイルを「Pekko 由来」「自分で書いた」に機械判定し、TSV と集計を出す。

原典は Pekko v2.0.0（commit 9fbd2283b1cf701c90a1223bf621942c191db61f）。判定材料は、上から順に
当たった 1 つを根拠にする（材料の一覧は docs/design/issue-206-license-policy.md §3.3）。

  1. 冒頭 20 行の移植マーカー。`tools/license-headers/apply_ported_headers.py` と同じ定義。
  2. 原典への対応の記述（`Mirrors \\`org.apache.pekko.X\\``、原典パス、Pekko に触れる `Ported from`）。
  3. 台帳（ledger/nodes.yaml と retired）の rust.test_file。
  4. 原典に同名のファイル・型があるか、Pekko への言及だけがあるもの（要確認）。
  5. PR #215 の設計レビュー 2 回目の H1 の規則（crates のみ、要確認）。型名を大文字小文字を
     区別せずに照合する、コメントの `Type.method`、「corresponding to」「the ported」
     「original `X`」と行をまたぐ「the original」。加えて、Pekko が使う第三者のコード
     （scala-library、hash-prospector）への言及（規則 4）。

方針：判定は 2 値。「迷うもの」は Pekko 由来へ倒し、要確認の印と理由を付ける。
人の決定（docs/design/issue-206-origin-decisions.tsv）は機械の判定の後に重ね、列
`付けるSPDX` と `決定の出典` に出す。`MIT OR Apache-2.0` はファイルごとの決定でしか付かない。
決定の無い「自分で書いた」は `保留` である（元帥の決定 1、2026-11-26）。ただし
`保留` は、既存のファイルで出どころが決まらないものの区分である。新しく足す
ファイルは、自作も含めて、まず `Apache-2.0` とする（元帥の規則、2026-11-26）。
そのため、前の一覧に無い crates のファイルには「要確認」の印を付け、決定 2 の
一括の決定で `Apache-2.0` と出す。印は次回以降の作り直しでも引き継ぐ。

crates の Pekko 由来の行には、ヘッダーへ表示を写す元の `原典ファイル` と、原典のファイル先頭
にある第三者の表示（`第三者の表示`）も出す。

使い方（リポジトリのルートで実行する。引数はすべて省略できる）:
    uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py

    --repo        既定はこのスクリプトから 2 つ上（リポジトリのルート）
    --pekko       原典の git リポジトリ。既定は環境変数 PEKKO_REPO、無ければ <repo>/pekko
                  （tools/pekko-source/prepare_pekko_source.sh が用意する）
    --decisions   人の決定。既定は docs/design/issue-206-origin-decisions.tsv
    --out-tsv     既定は docs/design/issue-206-file-origin.tsv
    --out-summary 指定したときだけ集計の JSON を書く

対象の集合は `git ls-files` で決める（追跡していないファイルは入らない）。出力の TSV は、
このスクリプトと決定のファイルから作り直せる。ライセンス作業のためのファイル
（SELF_EXCLUDE_FILES、SELF_EXCLUDE_DIRS）は走査しない。
"""
import argparse
import json
import os
import subprocess
import sys
from collections import Counter
from pathlib import Path

from header_core import SPDX_APACHE, notice_hash, strip_header_for_scan
from notice_render import render_pekko_notice_lines
from origin_decisions import DECISIONS_DEFAULT, apply_decisions, load_decisions
from origin_evidence import H1_KINDS, classify, evidence_path
from origin_pekko import open_pekko, source_files, third_party_evidence

RELOCATE_TABLE = Path("docs") / "baseline" / "v2.0.0-port-targets.tsv"
MOVED_ID_MASK = "<移動前の id>"
TSV_HEADER = [
    "path",
    "判定",
    "根拠の種類",
    "根拠の中身",
    "要確認",
    "要確認の理由",
    "走査範囲",
    "付けるSPDX",
    "決定の出典",
    "原典ファイル",
    "第三者の表示",
    "表示のハッシュ",
]

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
    "docs/design/issue-206-origin-decisions.tsv",
    "docs/design/issue-206-mit-candidates.md",
    "docs/design/issue-206-known-exceptions.md",
}

SELF_EXCLUDE_DIRS = ("tools/license-headers/",)

BINARY_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".pdf", ".jar", ".zip", ".gz", ".lock",
    ".woff", ".woff2", ".ttf", ".so", ".dylib", ".rlib", ".bin",
}

MAX_BYTES = 2 * 1024 * 1024

def read_text(path):
    with path.open("r", encoding="utf-8", errors="replace", newline="") as handle:
        return handle.read()

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

def target_files(repo):
    """対象の集合は `git ls-files` で決める（L7。手元と CI で同じ集合になり、追跡していない
    ファイルは入らない）。crates/** の *.rs は全件で、名前によって飛ばすディレクトリは無い。
    tools/ scripts/ docs/ とトップの一部は、判定材料が 1 つでも当たったものだけを返す。"""
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z"], check=True, capture_output=True, text=True
    )
    crates = []
    others = []
    for posix in sorted(name for name in out.stdout.split("\0") if name):
        path = repo / posix
        if not path.is_file():
            raise SystemExit(f"追跡しているのに作業ツリーに無いファイル: {posix}")
        parts = Path(posix).parts
        if parts[0] == "crates":
            if path.suffix == ".rs":
                crates.append(posix)
            continue
        if any(part in SKIP_DIRS for part in parts):
            continue
        if posix in SELF_EXCLUDE_FILES or posix.startswith(SELF_EXCLUDE_DIRS):
            continue
        if path.suffix in BINARY_SUFFIXES or path.stat().st_size > MAX_BYTES:
            continue
        others.append(posix)
    return crates, others


# 新規ファイルの印の理由。この文面が前の一覧に残っている行は、次回以降も
# 新規ファイルとして扱い、印を引き継ぐ（作り直しで `保留` へ戻らないように）。
NEW_FILE_REASON = "新規ファイル：前の一覧に無いファイルは、自作も含めて Apache-2.0 とする（元帥の規則、2026-11-26）"


def previous_rows(path):
    """既存の出力 TSV を path → (要確認, 要確認の理由) で読む。ファイルが無ければ空。"""
    if not path.is_file():
        return {}
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        return {}
    header = lines[0].split("\t")
    path_i = header.index("path")
    flag_i = header.index("要確認")
    reason_i = header.index("要確認の理由")
    out = {}
    for line in lines[1:]:
        if not line:
            continue
        cells = line.split("\t")
        out[cells[path_i]] = (cells[flag_i], cells[reason_i])
    return out


def mark_new_files(rows, previous):
    """新規ファイルに「要確認」の印を付け、決定 2 の一括で `Apache-2.0` と出す。

    対象は crates の行だけである。前の一覧に無い行が新規ファイルであり、
    前の行に新規ファイルの印の理由が残っている行は印を引き継ぐ。既に別の
    理由で印が付いている行は、そのままにする（一括の決定は同じく当たる）。
    """
    marked = []
    for rel, scope, verdict, kind, snippet, flag, reason in rows:
        if scope == "crates" and not flag:
            before = previous.get(rel)
            if before is None or before[1] == NEW_FILE_REASON:
                flag, reason = "要確認", NEW_FILE_REASON
        marked.append((rel, scope, verdict, kind, snippet, flag, reason))
    return marked


def build_rows(repo, ledger, pekko):
    """判定の行と、crates のファイルの本文を返す。"""
    crates, others = target_files(repo)
    rows = []
    texts = {}
    for rel in crates:
        texts[rel] = strip_header_for_scan(read_text(repo / rel))
        rows.append((rel, "crates") + classify(rel, texts[rel], "crates", ledger, pekko))
    for rel in others:
        verdict = classify(rel, read_text(repo / rel), "other", ledger, pekko)
        if verdict[1] != "なし":
            rows.append((rel, "other") + verdict)
    return rows, texts


def build_origins(rows, texts, ledger, pekko):
    """crates の Pekko 由来の行について、ヘッダーへ表示を写す原典のファイルと第三者の表示。"""
    origins = {}
    derived = set()
    for rel, scope, verdict, kind, snippet, _flag, _reason in rows:
        if scope != "crates" or verdict != "Pekko 由来":
            origins[rel] = ([], [])
            continue
        derived.add(rel)
        hinted = [path for path in (evidence_path(snippet), snippet if kind == "原典同名" else None) if path]
        sources = source_files(texts[rel], ledger.get(rel), hinted, pekko)
        origins[rel] = (sources, third_party_evidence(texts[rel], sources, pekko.notices))
    for rel in sorted(derived):
        root = str(Path(rel).parent) + ".rs"
        if root in derived:
            origins[rel] = (origins[rel][0], inherit_notices(origins[rel][1], origins[root][1]))
    return origins


def inherit_notices(own, root):
    """モジュールの根（`D.rs`）が原典のファイル先頭から写す第三者の表示を、`D/` の下の
    Pekko 由来のファイルにも足す。モジュールの一部だけを写したファイルは、本文に原典の名前を
    書かないことがある（`frequency_sketch/counter_bits.rs` など）。写しすぎても害は無い。"""
    from_source = [label for label in root if label.endswith((".scala）", ".java）"))]
    return own + [label for label in from_source if label not in own]


def moved_ids(repo):
    """台帳の根拠表で移動した旧 id。一覧に書くと `relocate_nodes.py check` が
    「旧 id の参照が残っている」で落とすので、根拠の中身から伏せる。"""
    sys.path.insert(0, str(repo / "ledger" / "tools"))
    from relocate_table import read_table

    return sorted({row.old_id for row in read_table(repo / RELOCATE_TABLE) if row.kind == "move"}, key=len, reverse=True)


def mask_moved(text, moved):
    for old_id in moved:
        text = text.replace(old_id, MOVED_ID_MASK)
    return text


def build_notice_hashes(rows, decided, origins, pekko):
    """C6 用の `表示のハッシュ` を計算する（crates の Apache-2.0 / 外部式の行のみ）。"""
    hashes = {}

    def read_source(rel_path):
        return (pekko.tree / rel_path).read_text(encoding="utf-8", errors="replace")

    def resolve_third_party(owner, filename):
        return pekko.third_party_blocks.get((owner, filename), [])

    for rel, scope, _verdict, _kind, _snippet, _flag, _reason in rows:
        spdx = decided[rel][0]
        if scope == "crates" and (spdx == SPDX_APACHE or spdx.startswith(f"{SPDX_APACHE} AND ")):
            sources, third = origins[rel]
            lines = render_pekko_notice_lines(sources, third, read_source, resolve_third_party)
            hashes[rel] = notice_hash(lines)
        else:
            hashes[rel] = ""
    return hashes


def write_tsv(path, rows, decided, origins, hashes, moved):
    lines = ["\t".join(TSV_HEADER)]
    for rel, scope, verdict, kind, snippet, flag, reason in rows:
        spdx, source = decided[rel]
        sources, third = origins[rel]
        cells = [rel, verdict, kind, mask_moved(snippet.replace("\t", " "), moved), flag, reason, scope]
        lines.append("\t".join(cells + [spdx, source, ";".join(sources), ";".join(third), hashes[rel]]))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def summarize(rows, decided, origins, ledger):
    def counts(values):
        return dict(Counter(values))

    crates = [row for row in rows if row[1] == "crates"]
    others = [row for row in rows if row[1] == "other"]
    return {
        "rows": len(rows),
        "crates_rs": len(crates),
        "others_listed": len(others),
        "verdict_crates": counts(row[2] for row in crates),
        "verdict_others": counts(row[2] for row in others),
        "flag_crates": counts(row[5] or "(なし)" for row in crates),
        "flag_others": counts(row[5] or "(なし)" for row in others),
        "kind": counts(row[3] for row in rows),
        "reason": counts(row[6] for row in rows if row[6]),
        "spdx_crates": counts(decided[row[0]][0] for row in crates),
        "spdx_others": counts(decided[row[0]][0] for row in others),
        "h1_rows": sorted(row[0] for row in rows if row[3] in H1_KINDS),
        "sources_unresolved_crates": sum(1 for row in crates if row[2] == "Pekko 由来" and not origins[row[0]][0]),
        "third_party_rows": {row[0]: origins[row[0]][1] for row in rows if origins[row[0]][1]},
        "ledger_test_files": len(ledger),
        "ledger_test_files_not_listed": sorted(rel for rel in ledger if not any(r[0] == rel for r in rows)),
    }


def parse_args(argv):
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
        "--decisions",
        type=Path,
        default=DECISIONS_DEFAULT,
        help="人の決定のファイル（既定は docs/design/issue-206-origin-decisions.tsv。相対ならリポジトリのルートから）",
    )
    parser.add_argument(
        "--out-tsv",
        type=Path,
        default=repo_default / "docs" / "design" / "issue-206-file-origin.tsv",
        help="一覧の TSV（既定は docs/design/issue-206-file-origin.tsv）",
    )
    parser.add_argument("--out-summary", type=Path, default=None, help="集計の JSON（指定したときだけ書く）")
    parser.add_argument("--pekko-tree", type=Path, default=None, help="原典の展開先（省略時は一時ディレクトリ）")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    repo = args.repo.resolve()
    decisions_path = args.decisions if args.decisions.is_absolute() else repo / args.decisions
    with open_pekko(args.pekko.resolve(), args.pekko_tree) as pekko:
        ledger = ledger_index(repo)
        rows, texts = build_rows(repo, ledger, pekko)
        rows = mark_new_files(rows, previous_rows(args.out_tsv))
        decided = apply_decisions(rows, load_decisions(decisions_path), decisions_path.name)
        origins = build_origins(rows, texts, ledger, pekko)
        hashes = build_notice_hashes(rows, decided, origins, pekko)
    write_tsv(args.out_tsv, rows, decided, origins, hashes, moved_ids(repo))
    summary = summarize(rows, decided, origins, ledger)
    if args.out_summary:
        args.out_summary.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0

if __name__ == "__main__":
    sys.exit(main())
