#!/usr/bin/env python3
"""knowledge `fraktor-porting-rules` がプロンプト本文に収まることを確かめる。

使い方:
    uv run --with pyyaml python3 tools/takt-facets/knowledge_budget_test.py

takt は 1 ステップの knowledge を連結し、先頭 2,000 字だけをプロンプト本文に載せる
（docs/decisions/takt-workflow-design.md「facet の並び順」）。fraktor-porting-rules は
knowledge の先頭に置くので、facet 全体が 2,000 字以内なら全文が本文に載る。超えると
末尾の規約から本文に載らなくなる。複数の PR の追記が合わさって超えることがあるので
（#165 と #166 の合成で 2,274 字になった）、マージ後の facet 全体を測る。
"""
import sys
import traceback
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[2]
FACET = REPO / ".takt" / "facets" / "knowledge" / "fraktor-porting-rules.md"
WORKFLOWS = REPO / ".takt" / "workflows"
NAME = "fraktor-porting-rules"
BUDGET = 2000

# 本文から落ちてはいけない規約（テスト先行、テストファイルの凍結、テストの弱体化の禁止、
# 人間だけが決めること）。
REQUIRED = (
    "テスト先行。red を確認していない実装は書かない",
    "実装ステップはテストファイルを編集しない",
    "移植したテストを弱めない",
    "人間だけが決める",
    "`do_not_port` も人間が status に直接書く",
    "`not_built`（作らない区分）も人間だけが決める",
    "Pekko v2.0.0",
)


def knowledge_lists(node, key=""):
    """workflow の YAML から、キー名に knowledge を含むリストを集める。"""
    if isinstance(node, dict):
        for k, v in node.items():
            yield from knowledge_lists(v, str(k))
    elif isinstance(node, list):
        if "knowledge" in key and all(isinstance(x, str) for x in node):
            yield key, node
        for item in node:
            yield from knowledge_lists(item, key)


def test_facet_fits_the_prompt_budget() -> None:
    text = FACET.read_text(encoding="utf-8")
    assert len(text) <= BUDGET, f"{FACET.name} is {len(text)} characters, over the {BUDGET}-character budget"


def test_required_rules_are_inside_the_budget() -> None:
    head = FACET.read_text(encoding="utf-8")[:BUDGET]
    missing = [r for r in REQUIRED if r not in head]
    assert not missing, f"rules outside the first {BUDGET} characters: {missing}"


def test_porting_rules_come_first_wherever_they_are_used() -> None:
    uses = []
    for path in sorted(WORKFLOWS.glob("*.yaml")):
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
        for key, items in knowledge_lists(doc):
            if NAME in items:
                uses.append((path.name, key, items))
    assert uses, "no workflow uses fraktor-porting-rules"
    wrong = [u for u in uses if u[2][0] != NAME]
    assert not wrong, f"{NAME} must be the first knowledge entry: {wrong}"


def main() -> int:
    tests = [
        test_facet_fits_the_prompt_budget,
        test_required_rules_are_inside_the_budget,
        test_porting_rules_come_first_wherever_they_are_used,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except Exception:
            failures += 1
            print(f"FAIL {test.__name__}:")
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
