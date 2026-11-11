#!/usr/bin/env python3
"""baseline 台帳（モジュール粒度）を、リポジトリ台帳スキーマ（テストクラス粒度）へ変換する。

使い方:
    python3 convert_baseline.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml \
        [--module actor-tests] [--limit N] > out.yaml

id は Pekko リポジトリ相対のファイルパスで、一意キーである。name は表示名で、一意ではない。
変換できない欄（triage、needs、kind の fixture 判定、cases）は既定値か null（未抽出）のまま出す。
id が重複するか、id が所属モジュールのディレクトリで始まらないときは、何も出さずに失敗する。
詳細は docs/decisions/ledger-schema-draft.md を参照。
"""
import argparse
import collections
import sys

import yaml


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--module", default=None, help="directory 名で1モジュールに絞る")
    ap.add_argument("--limit", type=int, default=None, help="先頭 N クラスだけ出す")
    args = ap.parse_args()

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
