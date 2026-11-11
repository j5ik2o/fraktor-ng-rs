#!/usr/bin/env python3
"""syn-check の JSON 出力を、ルール別に「テスト側のパス」と「それ以外」に分けて数える。

使い方（旧 fraktor-rs のコーパスでの例）:
    cd <fraktor-rs の checkout>
    <syn-check> --layout legacy modules | python3 <このスクリプト>

テスト側のパスとは、旧 CI が lint の対象にしていなかったコードの近似である。
tests.rs、*_test.rs、*_tests.rs と、tests、benches、examples ディレクトリの下を指す。
「それ以外」の違反は、ファイルと行を 1 件ずつ出す。
"""
import collections
import json
import sys


def is_test_side(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    if name == "tests.rs" or name.endswith(("_test.rs", "_tests.rs")):
        return True
    return any(f"/{directory}/" in f"/{path}" for directory in ("tests", "benches", "examples"))


def main() -> int:
    report = json.load(sys.stdin)
    summary = report["summary"]
    print(f"files {summary['files_checked']}, parse failures {summary['parse_failures']}, "
          f"violations {summary['violations']}, elapsed {summary['elapsed_ms']} ms")
    by_rule = collections.defaultdict(list)
    for violation in report["violations"]:
        by_rule[violation["rule"]].append(violation)
    print("| ルール | 件数 | テスト側のパス | それ以外 |")
    print("|---|---|---|---|")
    totals = [0, 0, 0]
    for rule in report["rules"]:
        violations = by_rule.get(rule, [])
        test_side = sum(1 for violation in violations if is_test_side(violation["path"]))
        row = [len(violations), test_side, len(violations) - test_side]
        totals = [total + value for total, value in zip(totals, row)]
        print(f"| `{rule}` | {row[0]} | {row[1]} | {row[2]} |")
    print(f"| 合計 | {totals[0]} | {totals[1]} | {totals[2]} |")
    print()
    for rule in report["rules"]:
        for violation in by_rule.get(rule, []):
            if not is_test_side(violation["path"]):
                print(f"{rule}: {violation['path']}:{violation['line']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
