#!/usr/bin/env python3
"""基準タグの切り替え後に、判定済みノードの照合済みタグ（verified_against）を現行タグへ揃える道具。

台帳の書き換えは `ledger_lines.py`（verify_red の mark_green と同じ行単位の道具）を使う。
変えるのは対象ノードの最上位の `verified_against` の値だけで、ほかの欄・行頭の注記・最上位の
`retired`（退役したノード）には触れない。1 回の起動で、指定した id の行だけを書き換える。

使い方:
    # 現行タグで未照合の判定済みノードの id を出す（apply の入力の候補になる）
    python3 ledger/tools/reverify_against.py plan --ledger ledger/nodes.yaml

    # 実際に原典と突き合わせて理由が成り立つと確かめたノードの id だけを書いたファイルを渡す
    # （plan の出力をそのまま apply に流さない。確かめていないノードを現行タグにしない）
    python3 ledger/tools/reverify_against.py apply --ledger ledger/nodes.yaml \
        --ids /tmp/checked-ids.txt [--dry-run]

    # 判定済みノードの原典の有無と、過去の基準からの行数差を表にする（PyYAML を使う）
    uv run --with pyyaml python3 ledger/tools/reverify_against.py report \
        --ledger ledger/nodes.yaml --pekko pekko [--status do_not_port] [--unverified-only]

照合済みタグは、基準ファイル（tools/pekko-source/pekko-baseline.env）の現行タグに揃える。
`--tag` を省くと現行タグを使い、現行タグでない値を渡すと拒否する（過去のタグを書くと、照合済みの
ノードが「現行タグで未照合」に戻り、台帳の検査は過去のタグを有効とするため気づけない）。

規則（`docs/decisions/ledger-schema-draft.md` の verified_against）:
  - 照合済みタグを書けるのは判定済みの status（green / do_not_port / not_built）だけである。
    それ以外の status のノードは欄を持たないか null なので、apply は拒否する
  - 照合していないノードを現行タグに書き換えない。apply は id を明示した分だけを書き換える
  - report は原典を `pekko_baseline.source_tag` で検査してから表を作る（未準備の `pekko/` や、
    別の版をチェックアウトした原典では、ファイルの有無を数えずに失敗する）
  - PyYAML は report のときだけ遅延 import する（apply と plan は PyYAML なしで動く）
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path

from ledger_lines import _set_top_level_key

# 判定済み（原典と照合して決めた）status。pekko_baseline.py の JUDGED_STATUSES と同じ。
JUDGED_STATUSES = ("green", "do_not_port", "not_built")

FIELD = "verified_against"

TOP_LEVEL_FIELD = re.compile(r"  ([A-Za-z_][\w-]*):\s*(.*)$")


def _baseline():
    import pekko_baseline

    return pekko_baseline.load()


def _resolve_tag(value: str | None) -> str:
    """現行タグを返す。value が現行タグでなければ ValueError（過去のタグを書かせない）。"""
    current = _baseline().tag
    if value is None:
        return current
    if value != current:
        raise ValueError(f"--tag {value!r} は現行の基準タグ {current!r} でない（照合済みタグは現行タグに揃える）")
    return value


def read_field(lines: list[str], key: str, span: tuple[int, int]) -> str | None:
    """対象ノードの最上位の欄（列 2 の `key:`）の値を返す。欄が無ければ None。"""
    for i in range(span[0] + 1, span[1]):
        match = TOP_LEVEL_FIELD.match(lines[i])
        if match and match.group(1) == key:
            value = re.split(r"\s+#", match.group(2), maxsplit=1)[0].strip()
            return value
    return None


def _span(lines: list[str], node_id: str) -> tuple[int, int]:
    from ledger_lines import node_span

    span = node_span(lines, node_id)
    if span is None:
        raise ValueError(f"{node_id}: nodes に無い（退役したノードや、id の誤り）")
    return span


def _status(lines: list[str], span: tuple[int, int]) -> str | None:
    return read_field(lines, "status", span)


def set_tag(lines: list[str], node_id: str, tag: str) -> list[str]:
    """1 ノードの verified_against を tag にした行を返す（判定済みの status でなければ ValueError）。"""
    span = _span(lines, node_id)
    status = _status(lines, span)
    if status not in JUDGED_STATUSES:
        raise ValueError(f"{node_id}: status {status!r} は {'/'.join(JUDGED_STATUSES)} でない")
    return _set_top_level_key(lines, node_id, FIELD, tag)


def _judged(lines: list[str]) -> list[tuple[str, str | None]]:
    """nodes の判定済みノードの (id, 現在の verified_against) を、台帳の順で返す。"""
    from ledger_lines import line_sections

    sections = line_sections(lines)
    result = []
    for i, line in enumerate(lines):
        if sections[i] != "nodes" or not line.startswith("- id: "):
            continue
        node_id = line.split(":", 1)[1].strip()
        span = _span(lines, node_id)
        status = _status(lines, span)
        if status in JUDGED_STATUSES:
            result.append((node_id, read_field(lines, FIELD, span)))
    return result


def _outside(lines: list[str], spans) -> list[str]:
    """ノードの範囲の外にある行を、順序を保って返す。"""
    inside = set()
    for span in spans:
        inside.update(range(span[0], span[1]))
    return [line for i, line in enumerate(lines) if i not in inside]


def _read_ids(path: str) -> list[str]:
    ids = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            ids.append(line)
    return ids


def cmd_plan(args) -> int:
    tag = _resolve_tag(args.tag)
    lines = Path(args.ledger).read_text(encoding="utf-8").splitlines(keepends=True)
    pending = [node_id for node_id, current in _judged(lines) if current != tag]
    for node_id in pending:
        print(node_id)
    print(f"{tag} で未照合の判定済みノード: {len(pending)} 件", file=sys.stderr)
    return 0


def cmd_apply(args) -> int:
    tag = _resolve_tag(args.tag)
    path = Path(args.ledger)
    original = path.read_text(encoding="utf-8").splitlines(keepends=True)
    ids = _read_ids(args.ids)
    if not ids:
        print(f"{args.ids}: 対象の id が空", file=sys.stderr)
        return 1
    if len(set(ids)) != len(ids):
        print(f"{args.ids}: id が重複している", file=sys.stderr)
        return 1
    from ledger_lines import node_span

    lines = list(original)
    changed, same = [], []
    before_spans = {}
    for node_id in ids:
        try:
            span = _span(lines, node_id)
            if read_field(lines, FIELD, span) == tag:
                same.append(node_id)
                continue
            before_spans[node_id] = node_span(original, node_id)
            lines = set_tag(lines, node_id, tag)
        except ValueError as error:
            print(error, file=sys.stderr)
            print("台帳は書き換えていない", file=sys.stderr)
            return 1
        changed.append(node_id)
    # 対象ノードの範囲の外の行が 1 行も変わっていないことを確かめる（欄を足すときは中が 1 行増える）。
    after_spans = {node_id: node_span(lines, node_id) for node_id in changed}
    if any(span is None for span in after_spans.values()):
        print("書き換えたあとにノードが見つからない。台帳は書き換えていない", file=sys.stderr)
        return 1
    before_outside = _outside(original, before_spans.values())
    after_outside = _outside(lines, after_spans.values())
    if before_outside != after_outside:
        print("対象ノードの外の行が変わった。台帳は書き換えていない", file=sys.stderr)
        return 1
    if args.dry_run:
        print(f"変更する: {len(changed)} 件、既に {tag}: {len(same)} 件（--dry-run なので書かない）")
        return 0
    path.write_text("".join(lines), encoding="utf-8")
    print(f"変更した: {len(changed)} 件、既に {tag}: {len(same)} 件")
    return 0


def _numstat(pekko: str, tag: str, historical: str, node_id: str) -> str:
    """過去の基準と現行の基準の間の、そのファイルの行数差（`+追加/-削除`）を返す。"""
    proc = subprocess.run(
        ["git", "-C", pekko, "diff", "--numstat", "--no-renames", historical, tag, "--", node_id],
        capture_output=True, text=True, check=False,
    )
    if proc.returncode != 0:
        detail = " ".join(proc.stderr.strip().splitlines()[:1])
        return f"測れない（{detail}）"
    rows = [line.split("\t") for line in proc.stdout.splitlines() if line.strip()]
    if not rows:
        return "変更なし"
    added, deleted = rows[0][0], rows[0][1]
    return f"+{added}/-{deleted}"


def cmd_report(args) -> int:
    import pekko_baseline
    import rollup

    tag = _resolve_tag(args.tag)
    baseline = _baseline()
    try:
        pekko_baseline.source_tag(baseline, args.pekko)
    except pekko_baseline.BaselineError as error:
        print(error, file=sys.stderr)
        return 1
    nodes = rollup.load_nodes(args.ledger)
    judged = [n for n in nodes if n.get("status") in JUDGED_STATUSES]
    if args.unverified_only:
        judged = [n for n in judged if n.get(FIELD) != tag]
    if args.status:
        wanted = set(args.status.split(","))
        judged = [n for n in judged if n.get("status") in wanted]
    pekko = Path(args.pekko)
    print(f"| id | status | {FIELD} | 原典 {tag} のパス | 過去の基準からの行数差 |")
    print("|:---|:---|:---|:---|:---|")
    missing = 0
    for node in judged:
        path = pekko / node["id"]
        if path.is_file():
            exists = f"あり（{len(path.read_text(encoding='utf-8').splitlines())} 行）"
        else:
            exists = "なし"
            missing += 1
        print(f"| `{node['id']}` | {node.get('status')} | {node.get(FIELD)} | {exists} | "
              f"{_numstat(pekko, tag, args.historical, node['id'])} |")
    print(f"\n対象 {len(judged)} 件、原典にファイルが無い {missing} 件", file=sys.stderr)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan", help="現行タグで未照合の判定済みノードの id を出す")
    p.add_argument("--ledger", default="ledger/nodes.yaml")
    p.add_argument("--tag", default=None, help=f"既定は基準ファイルの現行タグ（{FIELD} に書く値）")
    p.set_defaults(func=cmd_plan)
    p = sub.add_parser("apply", help="対象ノードの verified_against を現行タグへ揃える")
    p.add_argument("--ledger", default="ledger/nodes.yaml")
    p.add_argument("--tag", default=None, help="既定は基準ファイルの現行タグ")
    p.add_argument("--ids", required=True, help="1 行 1 id のファイル（空行と # の行は無視する）")
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(func=cmd_apply)
    p = sub.add_parser("report", help="原典の有無と行数差の表を出す（PyYAML を使う）")
    p.add_argument("--ledger", default="ledger/nodes.yaml")
    p.add_argument("--pekko", default="pekko")
    p.add_argument("--tag", default=None, help="既定は基準ファイルの現行タグ")
    p.add_argument("--historical", default=None,
                   help="行数差を測る過去の基準タグ（既定は基準ファイルの過去のタグの先頭）")
    p.add_argument("--status", default=None, help="status をカンマ区切りで絞る")
    p.add_argument("--unverified-only", action="store_true",
                   help="現行タグで未照合のノードだけにする（既定は判定済みの全件）")
    p.set_defaults(func=cmd_report)
    args = ap.parse_args()
    if args.command == "report" and not args.historical:
        historical = sorted(_baseline().historical)
        if not historical:
            print("過去の基準が基準ファイルに無い。--historical を渡す", file=sys.stderr)
            return 1
        args.historical = historical[0]
    try:
        return args.func(args)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
