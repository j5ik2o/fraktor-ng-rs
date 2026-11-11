#!/usr/bin/env python3
"""baseline 台帳（モジュール粒度）を、リポジトリ台帳スキーマ（テストクラス粒度）へ変換する。

使い方:
    python3 convert_baseline.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml \
        [--module actor-tests] [--limit N] > out.yaml

変換できない欄（needs.testkit / premise_flags のクラス単位の値）は null（未抽出）のまま出す。
詳細は docs/decisions/ledger-schema-draft.md を参照。
"""
import argparse
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
                    "id": f"{mod['directory']}:{tc['name']}",
                    "pekko": {
                        "module": mod["directory"],
                        "class": tc["name"],
                        "file": tc["file"],
                        "test_cases_approx": tc.get("test_cases_approx", 0),
                    },
                    "status": "unported",
                    "triage": {"kind": "pending", "reason": None},
                    "needs": {"testkit": None, "premise_flags": None},
                    "depends_on_impl": {
                        "modules": tc.get("referenced_modules", []),
                        "coverage": "approx",
                    },
                    "rust": {"crate": None, "test_file": None},
                    "goal": None,
                }
            )
            if args.limit and len(nodes) >= args.limit:
                break
        if args.limit and len(nodes) >= args.limit:
            break

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
