#!/usr/bin/env python3
"""verify_red.py の分類と台帳書き込みを、Isolation した一時ディレクトリで確かめる。

使い方:
    uv run --with pyyaml python3 ledger/tools/verify_red_test.py

verify_red.py は「直前の port-test 応答」から node/crate/test_file を読み、
対象テストを `cargo test` で実行して分類を返す。cargo と Pekko 原典は外部依存
なので、ここでは PATH の先頭にスタブ cargo を置き、PEKKO_ORIGINAL_ROOT に一時の
原典ツリーを用意し、`ledger/nodes.yaml` も一時ディレクトリに作って走らせる。
実リポジトリの台帳は書き換えない。

確かめる契約（計画の完了契約）:
  AG-CLASSIFY  移植が正しく実装が契約を満たす（全件成功 + 原典ケース数一致）とき
               classification は already_green になる
  AG-FALLBACK  原典が無い・ケース数不一致・0 件成功・失敗のときは unexpected_pass
               （または従来どおりの分類）で、台帳は green にしない
  AG-LEDGER    already_green のとき status: green、cases.total/ported/passing が実数、
               新しいキーを増やさない、excluded は書き換えない
  AG-REASON    already_green のとき、triage.reason が null/空なら `already_green: ` で始めて書き、
               既存の非 null な理由（do_not_port の人間の理由）は上書きしない（green_recorded: false）。
               新キーを増やさない。status は green のまま（do_not_port にしない）
  AG-MARK-RED  expected_* は従来どおり status: red（status 行だけを置換）
  AG-WORKFLOW  workflow の verify_red に already_green → equivalence-review の分岐があり、
               既存 4 分岐は残る（fallback の when(true) も含む）
  AG-LOOP      loop_monitors の cycle/threshold が変わっていない
  AG-PARSE     parse_passed は先頭の空 test result に引きずられず、対象テストの
               要約（最後の ok 行、または複数行の合計）から通過数を返す。
               失敗・0 件・不明は None
  AG-NONDEGRADE already_green で書いた台帳で rollup.py と check_needs.py が壊れない
"""
import json
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
VERIFY_RED = TOOLS / "verify_red.py"
ROLLUP = TOOLS / "rollup.py"
CHECK_NEEDS = TOOLS / "check_needs.py"
REPO = Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".takt" / "workflows" / "fraktor-port-node.yaml"

NODE_ID = "actor-tests/src/test/java/org/apache/pekko/actor/AddressTest.java"
CRATE = "fraktor-actor"
TEST_FILE = "crates/fraktor-actor/tests/address_test.rs"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def case_block(node_id: str, total, ported, passing, status: str = "unported") -> str:
    """台帳の1ノード分の YAML。mark_red / already_green が触る欄だけを持つ。"""
    def val(x):
        return "null" if x is None else x

    return (
        f"- id: {node_id}\n"
        f"  name: actor-tests:AddressTest\n"
        f"  kind: test\n"
        f"  pekko:\n"
        f"    module: actor-tests\n"
        f"    class: AddressTest\n"
        f"    test_cases_approx: 1\n"
        f"  status: {status}\n"
        f"  triage:\n"
        f"    kind: pending\n"
        f"    reason: null\n"
        f"  needs:\n"
        f"    testkit: []\n"
        f"    premise_flags: []\n"
        f"  depends_on_impl:\n"
        f"    modules: []\n"
        f"    coverage: approx\n"
        f"  cases:\n"
        f"    total: {val(total)}\n"
        f"    ported: {val(ported)}\n"
        f"    passing: {val(passing)}\n"
        f"    excluded: []\n"
        f"  rust:\n"
        f"    crate: {CRATE}\n"
        f"    test_file: {TEST_FILE}\n"
        f"  goal: G0d\n"
    )


def ledger_yaml(nodes: list[str]) -> str:
    return "nodes:\n" + "".join(nodes)


def java_original(cases: int) -> str:
    """原典テスト。@Test を cases 個だけ持つ。"""
    body = "".join(
        f"  @Test\n  public void case{i}() {{}}\n" for i in range(cases)
    )
    return (
        "package org.apache.pekko.actor;\n\n"
        "import org.junit.Test;\n\n"
        "public class AddressTest {\n"
        f"{body}"
        "}\n"
    )


CARGO_STUB_OK = """#!/bin/sh
# cargo test を模す。標準出力に cargo と同じ要約行を出す。
cat <<'OUT'
running 1 test
test ported_case ... ok

test result: ok. {passed} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
OUT
"""

CARGO_STUB_FAILED = """#!/bin/sh
cat <<'OUT'
running 1 test
test ported_case ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
OUT
"""

CARGO_STUB_COMPILE_ERROR = """#!/bin/sh
cat >&2 <<'OUT'
error[E0425]: cannot find value `missing` in this scope
OUT
exit 1
"""

CARGO_STUB_ZERO = """#!/bin/sh
cat <<'OUT'
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
OUT
"""

# 実 cargo の対象テスト実行で観測された形。先頭に 0 件の test result（別バイナリの
# 要約）が出て、続いて対象テストの要約が出る。最初の行を採ると None になる。
CARGO_STUB_OK_AFTER_EMPTY_SUMMARY = """#!/bin/sh
cat <<'OUT'
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running {passed} tests
test ported_case ... ok

test result: ok. {passed} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
OUT
"""

# 対象バイナリが複数あるとき、cargo はバイナリごとに test result を出す。
CARGO_STUB_MULTI_SUMMARY = """#!/bin/sh
cat <<'OUT'
running 1 test

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 2 tests

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
OUT
"""

# 最後の要約だけ失敗する形。合計では 0 通過になるので None でなければならない。
CARGO_STUB_LAST_FAILED = """#!/bin/sh
cat <<'OUT'
running 1 test

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 1 test
test ported_case ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
OUT
"""


def install_cargo(root: Path, body: str, passed: int = 0) -> Path:
    """PATH の先頭に置くスタブ cargo を作り、そのディレクトリを返す。"""
    bindir = root / "stub-bin"
    bindir.mkdir(parents=True, exist_ok=True)
    cargo = bindir / "cargo"
    cargo.write_text(body.format(passed=passed), encoding="utf-8")
    cargo.chmod(cargo.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return bindir


def make_scenario(root: Path, *, nodes_yaml: str, cargo_body: str, passed: int,
                  original_cases: int | None, with_original: bool) -> dict:
    """一時ワークスペースを作り、verify_red.py を走らせる準備をする。

    - ledger/nodes.yaml（一時）
    - スタブ cargo（PATH 先頭）
    - PEKKO_ORIGINAL_ROOT/<node id>（原典、with_original のとき）
    - 直前応答ファイル（node/crate/test_file の 3 行）
    """
    write(root / "ledger" / "nodes.yaml", nodes_yaml)
    bindir = install_cargo(root, cargo_body, passed)

    original_root = root / "pekko"
    if with_original and original_cases is not None:
        write(original_root / NODE_ID, java_original(original_cases))

    response = root / "previous-response.txt"
    write(response, f"node: {NODE_ID}\ncrate: {CRATE}\ntest_file: {TEST_FILE}\n")

    env = {k: v for k, v in os.environ.items() if k != "TAKT_PREVIOUS_RESPONSE_FILE"}
    env["TAKT_PREVIOUS_RESPONSE_FILE"] = str(response)
    env["PEKKO_ORIGINAL_ROOT"] = str(original_root)
    env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
    return {"root": root, "env": env}


def run_verify_red(scenario: dict) -> dict:
    proc = subprocess.run(
        [sys.executable, str(VERIFY_RED)],
        cwd=str(scenario["root"]), env=scenario["env"],
        capture_output=True, text=True, check=False,
    )
    assert proc.returncode == 0, (
        f"verify_red must always exit 0; got {proc.returncode}\n"
        f"stdout={proc.stdout}\nstderr={proc.stderr}"
    )
    return json.loads(proc.stdout)


def ledger_doc(root: Path) -> dict:
    import yaml
    return yaml.safe_load((root / "ledger" / "nodes.yaml").read_text(encoding="utf-8"))


def node_entry(root: Path, node_id: str = NODE_ID) -> dict:
    for n in ledger_doc(root).get("nodes", []):
        if n.get("id") == node_id:
            return n
    raise AssertionError(f"node not found: {node_id}")


def test_already_green_when_ported_and_satisfied():
    """全件成功 + 原典ケース数一致 → already_green、台帳は green。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=2,
            original_cases=2, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        node = node_entry(root)
        assert node["status"] == "green", node
        assert node["cases"]["total"] == 2, node["cases"]
        assert node["cases"]["ported"] == 2, node["cases"]
        assert node["cases"]["passing"] == 2, node["cases"]
        print("1. already_green (ported+satisfied):", result["classification"],
              "status =", node["status"], "cases =", node["cases"])


def test_unexpected_pass_when_original_missing():
    """原典が無い（移植の正しさを確かめられない）→ unexpected_pass、台帳は green にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=2,
            original_cases=None, with_original=False,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "unexpected_pass", result
        assert result.get("green_recorded") is False, result
        assert node_entry(root)["status"] == "unported", node_entry(root)
        print("2. unexpected_pass (no original):", result["classification"])


def test_unexpected_pass_when_case_count_mismatch():
    """原典ケース数と通過ケース数が一致しない → unexpected_pass、台帳は green にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=2,
            original_cases=3, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "unexpected_pass", result
        assert node_entry(root)["status"] == "unported", node_entry(root)
        print("3. unexpected_pass (count mismatch):", result["classification"])


def test_unexpected_pass_when_zero_cases_pass():
    """実行ケースが 0 件成功（契約充足を確かめられない）→ unexpected_pass。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_ZERO, passed=0,
            original_cases=0, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "unexpected_pass", result
        assert node_entry(root)["status"] == "unported", node_entry(root)
        print("4. unexpected_pass (zero passed):", result["classification"])


def test_expected_compile_error_marks_red():
    """未実装によるコンパイルエラー → expected_compile_error、status: red。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_COMPILE_ERROR, passed=0,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "expected_compile_error", result
        assert result["red_recorded"] is True, result
        assert node_entry(root)["status"] == "red", node_entry(root)
        print("5. expected_compile_error → red:", result["classification"])


def test_expected_assertion_marks_red():
    """アサーションで落ちる → expected_assertion、status: red。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_FAILED, passed=0,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "expected_assertion", result
        assert result["red_recorded"] is True, result
        assert node_entry(root)["status"] == "red", node_entry(root)
        print("6. expected_assertion → red:", result["classification"])


def test_mark_red_only_touches_status_line():
    """mark_red は status 行だけを置換し、cases などの他キーを書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, 3, 3, 3)])
        scenario = make_scenario(
            root,
            nodes_yaml=nodes_yaml,
            cargo_body=CARGO_STUB_COMPILE_ERROR, passed=0,
            original_cases=1, with_original=True,
        )
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8").splitlines()
        run_verify_red(scenario)
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8").splitlines()
        assert len(before) == len(after), (len(before), len(after))
        changed = [(b, a) for b, a in zip(before, after) if b != a]
        assert len(changed) == 1, changed
        old, new = changed[0]
        assert old.strip() == "status: unported", old
        assert new.strip() == "status: red", new
        node = node_entry(root)
        assert node["cases"]["total"] == 3, node["cases"]
        assert node["cases"]["ported"] == 3, node["cases"]
        assert node["cases"]["passing"] == 3, node["cases"]
        print("7. mark_red touches only status line")


def test_already_green_adds_no_new_keys():
    """already_green の書き込みで台帳の cases スキーマや他キーを増やさない（理由は triage.reason）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        node = node_entry(root)
        assert node["status"] == "green", node
        assert set(node["cases"].keys()) == {"total", "ported", "passing", "excluded"}, node["cases"]
        assert node["cases"]["excluded"] == [], node["cases"]
        assert set(node["triage"].keys()) == {"kind", "reason"}, node["triage"]
        assert node["triage"]["kind"] == "pending", node["triage"]
        print("8. already_green keeps cases schema:", sorted(node["cases"].keys()))


def test_workflow_has_already_green_branch_to_equivalence_review():
    """workflow の verify_red に already_green → equivalence-review の分岐があり、既存分岐も残る。"""
    import yaml
    doc = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    verify = next(s for s in doc["steps"] if s["name"] == "verify_red")
    conditions = [r["condition"] for r in verify["rules"]]
    target = [c for c in conditions if 'already_green' in c]
    assert target, conditions
    ag_rule = next(r for r in verify["rules"] if 'already_green' in r["condition"])
    assert ag_rule.get("next") == "equivalence-review", ag_rule
    joined = "\n".join(conditions)
    for expected in ("expected_compile_error", "expected_assertion",
                     "unexpected_pass", "unclassifiable"):
        assert expected in joined, (expected, conditions)
    # fallback の when(true) → ABORT も残す
    assert any(r["condition"] == "when(true)" and r.get("next") == "ABORT"
               for r in verify["rules"]), verify["rules"]
    print("9. workflow already_green →", ag_rule["next"], "(existing branches retained)")


def test_loop_monitors_unchanged():
    """loop_monitors の cycle/threshold が従来の定義のままである。"""
    import yaml
    doc = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    cycles = [m["cycle"] for m in doc["loop_monitors"]]
    assert ["port-test", "verify_red"] in cycles, cycles
    vr = next(m for m in doc["loop_monitors"] if m["cycle"] == ["port-test", "verify_red"])
    assert vr.get("ignore_steps") == ["classify-red"], vr
    assert vr.get("threshold") == 5, vr
    print("10. loop_monitors verify_red cycle/threshold unchanged")


def test_already_green_records_reason_in_triage_reason():
    """already_green のとき理由を既存の triage.reason に書き、新キーを増やさない（AG-REASON）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        node = node_entry(root)
        assert node["status"] == "green", node
        assert node["triage"]["kind"] == "pending", node["triage"]
        reason = node["triage"]["reason"]
        assert isinstance(reason, str) and reason.strip(), node["triage"]
        assert reason.startswith("already_green: "), node["triage"]
        assert set(node.keys()) == {
            "id", "name", "kind", "pekko", "status", "triage", "needs",
            "depends_on_impl", "cases", "rust", "goal",
        }, node.keys()
        assert set(node["triage"].keys()) == {"kind", "reason"}, node["triage"]
        print("12. already_green records triage.reason:", reason)


def test_existing_non_null_reason_is_preserved():
    """既存の非 null な triage.reason は already_green でも byte 不変・green_recorded: false（P4）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, None, None, None)]).replace(
            "    reason: null\n",
            "    reason: 以前の理由  # do_not_port のとき必須\n",
        )
        scenario = make_scenario(
            root,
            nodes_yaml=nodes_yaml,
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8").splitlines()
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is False, result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8").splitlines()
        before_reason = [l for l in before if l.strip().startswith("reason:")]
        after_reason = [l for l in after if l.strip().startswith("reason:")]
        assert before_reason == after_reason, (before_reason, after_reason)
        assert after_reason == ["    reason: 以前の理由  # do_not_port のとき必須"], after_reason
        node = node_entry(root)
        assert node["triage"]["reason"] == "以前の理由", node["triage"]
        assert node["status"] == "green", node
        print("13. existing non-null reason preserved:", node["triage"]["reason"])


def test_null_reason_is_replaced_by_already_green_reason():
    """triage.reason が null のときは理由を 1 行で書く（主操作の維持）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is True, result
        text = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        reason_lines = [l for l in text.splitlines() if l.strip().startswith("reason:")]
        assert len(reason_lines) == 1, reason_lines
        reason = node_entry(root)["triage"]["reason"]
        assert reason.startswith("already_green: "), reason
        print("13b. null reason replaced by single-line reason:", reason)


def test_empty_reason_is_replaced_by_already_green_reason():
    """triage.reason が空文字列でも null と同様に理由を書く（境界値）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, None, None, None)]).replace(
            "    reason: null\n",
            "    reason: ''\n",
        )
        scenario = make_scenario(
            root,
            nodes_yaml=nodes_yaml,
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is True, result
        reason = node_entry(root)["triage"]["reason"]
        assert isinstance(reason, str) and reason.startswith("already_green: "), reason
        print("13c. empty reason replaced:", reason)



def test_already_green_not_blocked_when_reason_key_missing():
    """triage.reason キーが無い台帳でも green にする（理由の欠落で classify を落とさない）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, None, None, None)]).replace(
            "    reason: null\n", "",
        )
        scenario = make_scenario(
            root,
            nodes_yaml=nodes_yaml,
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is True, result
        node = node_entry(root)
        assert node["status"] == "green", node
        print("14. already_green without reason key:", node["triage"])


def test_parse_passed_skips_leading_empty_summary():
    """先頭に 0 件の test result、続いて対象テストの要約がある出力で通過数を返す（AG-PARSE）。"""
    stub = CARGO_STUB_OK_AFTER_EMPTY_SUMMARY.replace("{passed}", "2")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=stub, passed=2,
            original_cases=2, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["passed"] == 2, result
        assert result["classification"] == "already_green", result
        print("15. parse_passed skips leading empty summary:", result["passed"])


def test_parse_passed_sums_multiple_summaries():
    """複数の test result があるとき、最後の1行だけに頼らず合計の通過数を返す（AG-PARSE）。"""
    stub = CARGO_STUB_MULTI_SUMMARY
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=stub, passed=3,
            original_cases=3, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["passed"] == 3, result
        assert result["classification"] == "already_green", result
        print("16. parse_passed sums summaries:", result["passed"])


def test_parse_passed_fails_when_last_summary_failed():
    """途中の要約が ok でも最後の要約が FAILED なら None で、already_green にしない（AG-PARSE）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_LAST_FAILED, passed=0,
            original_cases=1, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["passed"] is None, result
        assert result["classification"] != "already_green", result
        assert node_entry(root)["status"] != "green", node_entry(root)
        print("17. parse_passed returns None when last summary failed:",
              result["classification"])


def test_rollup_and_check_needs_do_not_break():
    """already_green で green にした台帳で、rollup.py と check_needs.py が壊れない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        run_verify_red(scenario)
        assert node_entry(root)["status"] == "green"

        caps = root / "ledger" / "capabilities.yaml"
        write(caps, "capabilities: []\n")

        rollup = subprocess.run(
            [sys.executable, str(ROLLUP), "--ledger", str(root / "ledger" / "nodes.yaml")],
            cwd=str(root), capture_output=True, text=True, check=False,
        )
        assert rollup.returncode == 0, (rollup.returncode, rollup.stdout, rollup.stderr)
        assert "green" in rollup.stdout.lower() or "G0d" in rollup.stdout, rollup.stdout

        needs = subprocess.run(
            [sys.executable, str(CHECK_NEEDS),
             "--ledger", str(root / "ledger" / "nodes.yaml"),
             "--capabilities", str(caps), "--goal", "G0d"],
            cwd=str(root), capture_output=True, text=True, check=False,
        )
        assert needs.returncode == 0, (needs.returncode, needs.stdout, needs.stderr)
        print("11. rollup/check_needs run on already_green ledger (exit 0)")


def main() -> int:
    tests = [
        test_already_green_when_ported_and_satisfied,
        test_unexpected_pass_when_original_missing,
        test_unexpected_pass_when_case_count_mismatch,
        test_unexpected_pass_when_zero_cases_pass,
        test_expected_compile_error_marks_red,
        test_expected_assertion_marks_red,
        test_mark_red_only_touches_status_line,
        test_already_green_adds_no_new_keys,
        test_workflow_has_already_green_branch_to_equivalence_review,
        test_loop_monitors_unchanged,
        test_already_green_records_reason_in_triage_reason,
        test_existing_non_null_reason_is_preserved,
        test_null_reason_is_replaced_by_already_green_reason,
        test_empty_reason_is_replaced_by_already_green_reason,
        test_already_green_not_blocked_when_reason_key_missing,
        test_parse_passed_skips_leading_empty_summary,
        test_parse_passed_sums_multiple_summaries,
        test_parse_passed_fails_when_last_summary_failed,
        test_rollup_and_check_needs_do_not_break,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except Exception:
            failures += 1
            print(f"FAIL {test.__name__}:")
            import traceback
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
