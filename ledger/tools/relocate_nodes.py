#!/usr/bin/env python3
"""原典 v2.0.0 でパスが変わったノードの id 付け替えと、原典で削除されたノードの記録を行う。

根拠表（`docs/baseline/v2.0.0-port-targets.tsv`。1 行 1 件）を入力にする。手で台帳を
書き換えず、この道具で適用する。台帳の行の書き換えは `ledger_lines.py` を通す。

- `move`：旧 id のノードを新 id へ改名する（`nodes` の中の `- id:` 1 行）。
  `status`・`cases`・`goal`・`needs`・件数（`pekko.test_cases_approx`）はそのまま残す。
  クラス名が変わる移動だけ `name` と `pekko.class` を新しいパスに合わせる。
  旧 id の文字列参照（ゴール文書の対象表、`ledger`、`docs`、`.takt`、`scripts`）も置き換える
- `delete`：そのノードを `status: do_not_port`、`triage.kind: do_not_port`、
  `triage.reason: "原典 v2.0.0 で削除（削除コミット <SHA>・確認日 <日付>）"`、
  `verified_against: <現行タグ>` にする。`retired` は使わない（後継が無い）

対象ノードの範囲の外の行（行頭の注記、ほかのノード、`retired`）は 1 行も変えない。
変えたら書き込まずに失敗する。対象ノードの中でも、上の欄以外の行が変われば失敗する。
PyYAML は `check` のときだけ遅延 import する（`apply` は PyYAML なしで動く）。

使い方:
    # 適用（冪等。既に適用済みの行は飛ばす。--dry-run は書かない）
    python3 ledger/tools/relocate_nodes.py apply

    # CI の検査。根拠表と台帳の整合と、旧 id の文字列参照 0 件を見る（PyYAML を使う）
    uv run --with pyyaml python3 ledger/tools/relocate_nodes.py check
"""
import argparse
import re
import sys
from pathlib import Path

from ledger_lines import _yaml_scalar, count_key, node_span, rename_id, set_nested_key, _set_top_level_key
from relocate_table import Row, find_references, read_table, replace_references

REPO = Path(__file__).resolve().parents[2]

DEFAULT_TABLE = "docs/baseline/v2.0.0-port-targets.tsv"
DEFAULT_LEDGER = "ledger/nodes.yaml"


def _module(path: str) -> str:
    return path.split("/", 1)[0]


def _class(path: str) -> str:
    return Path(path).stem


def _comparable(lines: list[str], span: tuple[int, int], drop: tuple[str, ...]) -> list[str]:
    """ノードの行から、書き換えてよい行（id・name・class など）を除いた残りを返す。"""
    kept = []
    for i in range(span[0], span[1]):
        line = lines[i]
        if i == span[0]:
            continue
        if any(re.match(pattern, line) for pattern in drop):
            continue
        kept.append(line)
    return kept


MOVE_DROP = (r"  name:", r"    class:")
DELETE_DROP = (r"  status:", r"    kind:", r"    reason:", r"  verified_against:")


def _apply_move(lines: list[str], row: Row) -> tuple[list[str], str | None]:
    original = lines
    before = node_span(original, row.old_id)
    lines = rename_id(original, row.old_id, row.new_id)
    module, klass = _module(row.new_id), _class(row.new_id)
    if _class(row.old_id) != klass:
        lines = _set_top_level_key(lines, row.new_id, "name", f"{module}:{klass}", quote=False)
        lines = set_nested_key(lines, row.new_id, "pekko", "class", klass)
    after = node_span(lines, row.new_id)
    if _comparable(original, before, MOVE_DROP) != _comparable(lines, after, MOVE_DROP):
        return lines, f"{row.old_id}: id・name・class 以外の行が変わった（status・cases・goal など）"
    return lines, None


def _read_line_field(lines: list[str], span: tuple[int, int], key: str, indent: str) -> str | None:
    """行の生の値（引用符を外さない）を返す。適用済みかの判定にだけ使う。"""
    for i in range(span[0] + 1, span[1]):
        if re.match(rf"{indent}{re.escape(key)}:", lines[i]):
            return lines[i].split(":", 1)[1].strip()
    return None


def _apply_delete(lines: list[str], row: Row, tag: str) -> tuple[list[str], str | None]:
    original = lines
    before = node_span(original, row.old_id)
    if before is None:
        return lines, f"{row.old_id}: nodes に無い"
    lines = _set_top_level_key(original, row.old_id, "status", "do_not_port")
    lines = set_nested_key(lines, row.old_id, "triage", "kind", "do_not_port")
    lines = set_nested_key(lines, row.old_id, "triage", "reason", row.delete_reason)
    lines = _set_top_level_key(lines, row.old_id, "verified_against", tag)
    after = node_span(lines, row.old_id)
    if _comparable(original, before, DELETE_DROP) != _comparable(lines, after, DELETE_DROP):
        return lines, f"{row.old_id}: status・triage・verified_against 以外の行が変わった"
    return lines, None


def _outside(lines: list[str], spans) -> list[str]:
    inside = set()
    for span in spans:
        inside.update(range(span[0], span[1]))
    return [line for i, line in enumerate(lines) if i not in inside]


def _paths(args) -> tuple[Path, Path, Path]:
    """(root, table, ledger) を返す。相対のパスは root からのものとして扱う。"""
    root = Path(args.root)
    table = Path(args.table)
    ledger = Path(args.ledger)
    return root, table if table.is_absolute() else root / table, ledger if ledger.is_absolute() else root / ledger


def _plan(original: list[str], rows: list[Row], tag: str) -> tuple[list[tuple[Row, tuple[int, int]]], list[str]]:
    """適用する行と、そのノードの元の行の範囲を返す（元の台帳だけで決める）。"""
    plan: list[tuple[Row, tuple[int, int]]] = []
    errors: list[str] = []
    for row in rows:
        span = node_span(original, row.old_id)
        if row.kind == "move":
            new_span = node_span(original, row.new_id)
            if span is None and new_span is not None:
                continue  # 適用済み
            if span is None:
                errors.append(f"{row.old_id}: nodes に無い")
            elif new_span is not None:
                errors.append(f"{row.new_id}: nodes に既にある（id を付け替えられない）")
            else:
                plan.append((row, span))
        else:
            if span is None:
                errors.append(f"{row.old_id}: nodes に無い")
                continue
            if count_key(original, row.old_id, "verified_against") > 1:
                errors.append(f"{row.old_id}: verified_against が二重にある（どちらが実値か決められない）")
                continue
            status = _read_line_field(original, span, "status", "  ")
            kind = _read_line_field(original, span, "kind", "    ")
            reason = _read_line_field(original, span, "reason", "    ")
            verified = _read_line_field(original, span, "verified_against", "  ")
            already = (status == "do_not_port" and kind == "do_not_port"
                       and reason in (row.delete_reason, _yaml_scalar(row.delete_reason))
                       and verified in (tag, f'"{tag}"'))
            if not already:
                plan.append((row, span))
    return plan, errors


def cmd_apply(args) -> int:
    import pekko_baseline

    tag = pekko_baseline.load().tag
    root, table, ledger = _paths(args)
    rows = read_table(table)
    original = ledger.read_text(encoding="utf-8").splitlines(keepends=True)
    plan, errors = _plan(original, rows, tag)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        print("台帳は書き換えていない", file=sys.stderr)
        return 1
    before_spans = [span for _, span in plan]
    lines = list(original)
    after_ids: list[str] = []
    moves, deletes = 0, 0
    for row, _ in plan:
        if row.kind == "move":
            lines, error = _apply_move(lines, row)
            after_ids.append(row.new_id)
            moves += 1
        else:
            lines, error = _apply_delete(lines, row, tag)
            after_ids.append(row.old_id)
            deletes += 1
        if error:
            print(error, file=sys.stderr)
            print("台帳は書き換えていない", file=sys.stderr)
            return 1
    after_spans = [node_span(lines, node_id) for node_id in after_ids]
    if any(span is None for span in after_spans):
        print("書き換えたあとにノードが見つからない。台帳は書き換えていない", file=sys.stderr)
        return 1
    if _outside(original, before_spans) != _outside(lines, after_spans):
        print("対象ノードの外の行が変わった。台帳は書き換えていない", file=sys.stderr)
        return 1
    ledger_changed = "".join(lines) != "".join(original)
    if not args.dry_run and ledger_changed:
        ledger.write_text("".join(lines), encoding="utf-8")
    replaced = replace_references(root, table.resolve(), rows, args.dry_run, ledger)
    verb = "置き換える" if args.dry_run else "置き換えた"
    print(f"台帳: move {moves} 件、delete {deletes} 件"
          + ("（--dry-run なので書かない）" if args.dry_run else ""))
    if replaced:
        print(f"旧 id の参照を{verb}:")
        for line in replaced:
            print(f"  {line}")
    else:
        print("旧 id の参照は 0 件")
    if args.dry_run:
        return 0
    remaining = find_references(root, table.resolve(), rows)
    if remaining:
        print(f"旧 id の参照が残っている（{len(remaining)} 行）:", file=sys.stderr)
        for line in remaining:
            print(f"  {line}", file=sys.stderr)
        return 1
    return 0


def _node_by_id(nodes: list[dict], node_id: str) -> dict | None:
    return next((n for n in nodes if n.get("id") == node_id), None)


def cmd_check(args) -> int:
    import pekko_baseline
    import rollup

    tag = pekko_baseline.load().tag
    root, table, ledger = _paths(args)
    rows = read_table(table)
    nodes = rollup.load_nodes(ledger)
    retired = rollup.load_retired(ledger)
    retired = retired if isinstance(retired, list) else []
    retired_ids = {r.get("id") for r in retired if isinstance(r, dict)}
    problems: list[str] = []
    for row in rows:
        if row.kind == "move":
            if _node_by_id(nodes, row.old_id) is not None:
                problems.append(f"{row.old_id}: 旧 id のノードが nodes に残っている")
            if row.old_id in retired_ids:
                problems.append(f"{row.old_id}: 旧 id が retired に残っている")
            node = _node_by_id(nodes, row.new_id)
            if node is None:
                problems.append(f"{row.new_id}: 新 id のノードが nodes に無い")
                continue
            module, klass = _module(row.new_id), _class(row.new_id)
            if node.get("name") != f"{module}:{klass}":
                problems.append(f"{row.new_id}: name が {module}:{klass} でない（{node.get('name')!r}）")
            pekko = node.get("pekko") if isinstance(node.get("pekko"), dict) else {}
            if pekko.get("class") != klass:
                problems.append(f"{row.new_id}: pekko.class が {klass} でない（{pekko.get('class')!r}）")
            if pekko.get("module") != module:
                problems.append(f"{row.new_id}: pekko.module が {module} でない（{pekko.get('module')!r}）")
        else:
            node = _node_by_id(nodes, row.old_id)
            if node is None:
                problems.append(f"{row.old_id}: ノードが nodes に無い")
                continue
            if node.get("status") != "do_not_port":
                problems.append(f"{row.old_id}: status が do_not_port でない（{node.get('status')!r}）")
            triage = node.get("triage") if isinstance(node.get("triage"), dict) else {}
            if triage.get("kind") != "do_not_port":
                problems.append(f"{row.old_id}: triage.kind が do_not_port でない（{triage.get('kind')!r}）")
            if triage.get("reason") != row.delete_reason:
                problems.append(f"{row.old_id}: triage.reason が {row.delete_reason!r} でない"
                                f"（{triage.get('reason')!r}）")
            if node.get("verified_against") != tag:
                problems.append(f"{row.old_id}: verified_against が {tag} でない"
                                f"（{node.get('verified_against')!r}）")
    for line in find_references(root, table.resolve(), rows):
        problems.append(f"旧 id の参照が残っている: {line}")
    if problems:
        print(f"根拠表と台帳の整合の違反 {len(problems)} 件:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    print(f"根拠表 {len(rows)} 件（move {sum(1 for r in rows if r.kind == 'move')}、"
          f"delete {sum(1 for r in rows if r.kind == 'delete')}）と台帳が一致し、"
          f"旧 id の参照は 0 件（基準 {tag}）")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(REPO))
    ap.add_argument("--table", default=DEFAULT_TABLE)
    ap.add_argument("--ledger", default=DEFAULT_LEDGER)
    sub = ap.add_subparsers(dest="command", required=True)
    p = sub.add_parser("apply", help="根拠表を台帳と参照へ適用する（冪等）")
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(func=cmd_apply)
    p = sub.add_parser("check", help="根拠表と台帳の整合、旧 id の参照 0 件を検査する")
    p.set_defaults(func=cmd_check)
    args = ap.parse_args()
    try:
        return args.func(args)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
