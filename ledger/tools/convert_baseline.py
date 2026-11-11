#!/usr/bin/env python3
"""baseline 台帳（モジュール粒度）を、リポジトリ台帳スキーマ（テストクラス粒度）へ変換する。

使い方:
    python3 convert_baseline.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml \
        [--module actor-tests] [--limit N] [--existing ledger/nodes.yaml] > out.yaml

id は Pekko リポジトリ相対のファイルパスで、一意キーである。name は表示名で、一意ではない。
変換できない欄（triage、needs、kind の fixture 判定、cases）は既定値か null（未抽出）のまま出す。
id が重複するか、id が所属モジュールのディレクトリで始まらないときは、何も出さずに失敗する。

--existing に既存の nodes.yaml を渡すと、状態保持モードになる。baseline 由来の欄
（name、pekko、depends_on_impl）は新しい baseline から作り直し、運用で育つ欄
（kind、status、triage、needs、cases、rust、goal）は id が一致する既存ノードから
そのまま引き継ぐ。状態を持つ既存ノード（status が unported でない、triage.kind が
pending でない、goal が設定済み、のいずれか）が新しい baseline に存在しない場合は、
黙って落とさず、その id を標準エラーへ出して失敗する。状態を持たない既存ノードの
消失は警告に留める。
詳細は docs/decisions/ledger-schema-draft.md を参照。
"""
import argparse
import collections
import sys

import yaml

# 運用で育つ欄。状態保持モードでは既存ノードから引き継ぐ
STATE_FIELDS = ("kind", "status", "triage", "needs", "cases", "rust", "goal")


def has_state(node: dict) -> bool:
    """状態保持の対象になる変更がノードにあるか。"""
    if node.get("status") not in (None, "unported"):
        return True
    triage = node.get("triage") or {}
    if triage.get("kind") not in (None, "pending"):
        return True
    if node.get("goal"):
        return True
    cases = node.get("cases") or {}
    if cases.get("total") is not None or cases.get("excluded"):
        return True
    return False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--module", default=None, help="directory 名で1モジュールに絞る")
    ap.add_argument("--limit", type=int, default=None, help="先頭 N クラスだけ出す")
    ap.add_argument("--existing", default=None, help="既存の nodes.yaml（状態保持モード）")
    args = ap.parse_args()

    existing_by_id = {}
    if args.existing:
        with open(args.existing, encoding="utf-8") as f:
            existing = yaml.safe_load(f) or {}
        for node in existing.get("nodes", []):
            existing_by_id[node["id"]] = node

    with open(args.baseline, encoding="utf-8") as f:
        data = yaml.safe_load(f)

    nodes = []
    for mod in data["modules"]:
        if args.module and mod["directory"] != args.module:
            continue
        for tc in mod.get("test_classes", []):
            nodes.append(
                {
                    "id": tc["file"],
                    "name": f"{mod['directory']}:{tc['name']}",
                    "kind": "test",
                    "pekko": {
                        "module": mod["directory"],
                        "class": tc["name"],
                        "test_cases_approx": tc.get("test_cases_approx", 0),
                    },
                    "status": "unported",
                    "triage": {"kind": "pending", "reason": None},
                    "needs": {"testkit": None, "premise_flags": None},
                    "depends_on_impl": {
                        "modules": tc.get("referenced_modules", []),
                        "coverage": "approx",
                    },
                    "cases": {"total": None, "ported": None, "passing": None, "excluded": []},
                    "rust": {"crate": None, "test_file": None},
                    "goal": None,
                }
            )
            if args.limit and len(nodes) >= args.limit:
                break
        if args.limit and len(nodes) >= args.limit:
            break

    if existing_by_id:
        new_ids = {n["id"] for n in nodes}
        carried = 0
        for node in nodes:
            old = existing_by_id.get(node["id"])
            if old is None:
                continue
            for field in STATE_FIELDS:
                if field in old:
                    node[field] = old[field]
            carried += 1
        vanished = [i for i in existing_by_id if i not in new_ids]
        vanished_with_state = [i for i in vanished if has_state(existing_by_id[i])]
        vanished_plain = [i for i in vanished if not has_state(existing_by_id[i])]
        print(
            f"状態保持: 既存 {len(existing_by_id)} 件中 {carried} 件を引き継ぎ、"
            f"新規 {len(nodes) - carried} 件",
            file=sys.stderr,
        )
        if vanished_plain:
            print(f"警告: 状態を持たない既存ノードが新しい baseline に無い: {vanished_plain}", file=sys.stderr)
        if vanished_with_state:
            print(f"状態を持つ既存ノードが新しい baseline に無い（黙って落とさない）: {vanished_with_state}", file=sys.stderr)
            return 1

    id_counts = collections.Counter(n["id"] for n in nodes)
    duplicated = sorted(i for i, count in id_counts.items() if count > 1)
    misplaced = [n["id"] for n in nodes if not n["id"].startswith(n["pekko"]["module"] + "/")]
    if duplicated or misplaced:
        print(f"id が重複: {duplicated}", file=sys.stderr)
        print(f"id が所属モジュールのディレクトリで始まらない: {misplaced}", file=sys.stderr)
        return 1

    yaml.safe_dump(
        {"nodes": nodes},
        sys.stdout,
        allow_unicode=True,
        sort_keys=False,
        width=120,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
