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
  AG-CLASSIFY  移植が正しく実装が契約を満たす（全件成功 + 原典ケース数と承認済み除外の
               件数の和が通過数に一致）とき classification は already_green になる
  AG-EXCLUDED  台帳の cases.excluded の件数を passed に足して原典件数と突き合わせ、
               green の保存は total=原典件数、ported=passing=passed。除外が無いときは
               従来と同じ結果になる
  AG-APPROVED  対象ノードの cases.excluded の各要素が、人間が記録する approved_by
               （空白以外の文字列）と approved_at（YYYY-MM-DD。引用なしの YAML 日付も可）を
               持つときだけ件数へ数える。欠落・空欄・null・不正な日付・不正な型が 1 件でも
               あれば count_excluded は None を返し、passed + excluded == original_cases の
               件数式が合っていても already_green にしない（unexpected_pass）。既存の
               name/reason だけの要素は未承認として拒否する（APPROVAL_CASES の表で確認）
               別ノードの引用なし不正日は対象ノードの判定に影響せず、対象の不正日だけが
               fail closed する（共有 SafeLoader は書き換えない）
  AG-COUNT     Scala のタグ付きケースも数え、`it should "..." in {` 形式も既知の書式として数える。
               既知の書式で全件を説明できない原典（WordSpec の `"name" must {` など。対応書式との
               混在を含む）は None にする
  AG-FALLBACK  原典が無い・ケース数不一致・0 件成功・失敗のときは unexpected_pass
               （または従来どおりの分類）で、台帳は green にしない
  AG-LEDGER    already_green のとき台帳は書き換えない（status は unported のまま）。green の
               永続化は record-already-green（record_already_green.py）が行う。cases は増減させない
  AG-REASON    already_green のとき stdout の reason が `already_green: ` で始まる。台帳の
               triage.reason と green の永続化は、verify_green と equivalence-review を通過後に
               record-already-green が呼ぶ mark_green が行う（AG-MARK-GREEN）
  AG-MARK-GREEN mark_green は status: green・cases.total/ported/passing・triage.reason
               （null/空のときだけ `already_green: `）を書き、既存の人間の理由は上書きしない
  AG-RESULT    already_green の判定結果（node/original_cases/passed/excluded/reason）を
               `.takt/already-green-result.json` へ書く（already_green でなければ classification のみ）。
               書込みに失敗したときは前回の結果を削除して終了コード 1 で失敗し、古い結果を
               record-already-green に読ませない
  AG-RECORD    record_already_green.py は result JSON が already_green のときだけ台帳の
               status/cases/triage.reason を書き、それ以外では台帳を変えない
  AG-MARK-RED  expected_* は従来どおり status: red（status 行だけを置換）
  AG-WORKFLOW  workflow の verify_red に already_green → verify_green の分岐があり、
               既存 4 分岐は残る（fallback の when(true) も含む）。verify_green は
               equivalence-review へ進む（already_green が機械ゲートを迂回しない）
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
from datetime import date as datetime_date
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
VERIFY_RED = TOOLS / "verify_red.py"
RECORD_ALREADY_GREEN = TOOLS / "record_already_green.py"
ROLLUP = TOOLS / "rollup.py"
CHECK_NEEDS = TOOLS / "check_needs.py"
REPO = Path(__file__).resolve().parents[2]
WORKFLOW = REPO / ".takt" / "workflows" / "fraktor-port-node.yaml"

NODE_ID = "actor-tests/src/test/java/org/apache/pekko/actor/AddressTest.java"
SCALA_NODE_ID = "actor-tests/src/test/scala/org/apache/pekko/actor/AddressSpec.scala"
CRATE = "fraktor-actor"
TEST_FILE = "crates/fraktor-actor/tests/address_test.rs"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


APPROVED_BY = "synthetic-human-approver"
APPROVED_AT = "2026-11-20"


def case_block(node_id: str, total, ported, passing, status: str = "unported",
               excluded: list[str] | None = None) -> str:
    """台帳の1ノード分の YAML。mark_red / already_green が触る欄だけを持つ。

    除外要素は name/reason に加えて、人による承認記録（approved_by/approved_at）を
    合成値で持つ。承認記録の妥当性は APPROVAL_CASES の表と _approved_item で確かめる。
    """
    def val(x):
        return "null" if x is None else x

    if excluded:
        excluded_yaml = "\n" + "".join(
            f"      - name: {name}\n"
            f"        reason: 承認済みの除外\n"
            f"        approved_by: {APPROVED_BY}\n"
            f"        approved_at: {APPROVED_AT}\n"
            for name in excluded
        )
    else:
        excluded_yaml = " []"

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
        f"    excluded:{excluded_yaml}\n"
        f"  rust:\n"
        f"    crate: {CRATE}\n"
        f"    test_file: {TEST_FILE}\n"
        f"  goal: G0d\n"
    )


OTHER_NODE_ID = "actor-tests/src/test/java/org/apache/pekko/actor/OtherTest.java"


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


def scala_original(in_cases: int, tagged_cases: int = 0, must_case: bool = False) -> str:
    """原典の Scala テスト。`"..." in {` と taggedAs つきの in を持つ。

    must_case のとき、既知のケース書式ではない `"name" must {` のブロックを足す。
    """
    body = "".join(f'  "{name}" in {{\n  }}\n' for name in [f"plain {i}" for i in range(in_cases)])
    body += "".join(
        f'  "{name}" taggedAs TimingTest in {{\n  }}\n'
        for name in [f"tagged {i}" for i in range(tagged_cases)]
    )
    if must_case:
        body += '  "A suite" must {\n  }\n'
    return (
        "package org.apache.pekko.actor\n\n"
        "import org.apache.pekko.testkit.PekkoSpec\n\n"
        "class AddressSpec extends PekkoSpec {\n"
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
                  original_cases: int | None, with_original: bool,
                  original_text: str | None = None,
                  node_id: str = NODE_ID) -> dict:
    """一時ワークスペースを作り、verify_red.py を走らせる準備をする。

    - ledger/nodes.yaml（一時）
    - スタブ cargo（PATH 先頭）
    - PEKKO_ORIGINAL_ROOT/<node id>（原典、with_original のとき）
    - 直前応答ファイル（node/crate/test_file の 3 行）
    """
    write(root / "ledger" / "nodes.yaml", nodes_yaml)
    bindir = install_cargo(root, cargo_body, passed)

    original_root = root / "pekko"
    if with_original and (original_text is not None or original_cases is not None):
        text = original_text if original_text is not None else java_original(original_cases)
        write(original_root / node_id, text)

    response = root / "previous-response.txt"
    write(response, f"node: {node_id}\ncrate: {CRATE}\ntest_file: {TEST_FILE}\n")

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
    """全件成功 + 原典ケース数一致 → already_green。台帳は書き換えない（green は ledger-update）。"""
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
        assert result["green_recorded"] is False, result
        node = node_entry(root)
        assert node["status"] == "unported", node
        assert node["cases"]["total"] is None, node["cases"]
        assert node["cases"]["ported"] is None, node["cases"]
        assert node["cases"]["passing"] is None, node["cases"]
        print("1. already_green (ported+satisfied):", result["classification"],
              "status =", node["status"], "cases =", node["cases"])


def test_already_green_writes_result_file():
    """already_green の判定結果を `.takt/already-green-result.json` へ書く（AG-RESULT）。"""
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
        saved = json.loads((root / ".takt" / "already-green-result.json").read_text(encoding="utf-8"))
        assert saved["classification"] == "already_green", saved
        assert saved["node"] == NODE_ID, saved
        assert saved["original_cases"] == 2 and saved["passed"] == 2, saved
        assert saved["excluded"] == 0, saved
        assert saved["reason"].startswith("already_green: "), saved
        print("1a. already_green writes result file:", saved["classification"])


def test_non_already_green_result_file_has_no_reason():
    """already_green でなければ result ファイルに classification だけを書き、record を no-op にする。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_FAILED, passed=0,
            original_cases=2, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "expected_assertion", result
        saved = json.loads((root / ".takt" / "already-green-result.json").read_text(encoding="utf-8"))
        assert saved["classification"] == "expected_assertion", saved
        assert saved["node"] == NODE_ID, saved
        print("1b. non-already_green result file:", saved["classification"])


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


def test_already_green_leaves_ledger_untouched():
    """already_green は台帳を書き換えない（byte 不変）。green の永続化は record-already-green。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=1, with_original=True,
        )
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert before == after
        node = node_entry(root)
        assert node["status"] == "unported", node
        assert set(node["cases"].keys()) == {"total", "ported", "passing", "excluded"}, node["cases"]
        assert set(node["triage"].keys()) == {"kind", "reason"}, node["triage"]
        print("8. already_green leaves ledger untouched:", node["status"])


def test_workflow_has_already_green_branch_to_verify_green():
    """workflow の verify_red に already_green → verify_green の分岐があり、既存分岐も残る。

    already_green が equivalence-review へ直行すると verify_green の機械ゲートを迂回する。
    遷移先は verify_green であり、verify_green は equivalence-review へ進む。
    """
    import yaml
    doc = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    verify = next(s for s in doc["steps"] if s["name"] == "verify_red")
    conditions = [r["condition"] for r in verify["rules"]]
    target = [c for c in conditions if 'already_green' in c]
    assert target, conditions
    ag_rule = next(r for r in verify["rules"] if 'already_green' in r["condition"])
    assert ag_rule.get("next") == "verify_green", ag_rule
    assert ag_rule.get("next") != "equivalence-review", ag_rule
    joined = "\n".join(conditions)
    for expected in ("expected_compile_error", "expected_assertion",
                     "unexpected_pass", "unclassifiable"):
        assert expected in joined, (expected, conditions)
    # fallback の when(true) → ABORT も残す
    assert any(r["condition"] == "when(true)" and r.get("next") == "ABORT"
               for r in verify["rules"]), verify["rules"]
    # verify_green は all_pass で equivalence-review へ進む（機械ゲートを迂回しない）
    vg = next(s for s in doc["steps"] if s["name"] == "verify_green")
    assert any(r.get("next") == "equivalence-review" for r in vg["rules"]), vg["rules"]
    # equivalence-review の通過後は record-already-green → ledger-update へ進む
    eq = next(s for s in doc["steps"] if s["name"] == "equivalence-review")
    assert any(r.get("next") == "record-already-green" for r in eq["rules"]), eq["rules"]
    rec = next(s for s in doc["steps"] if s["name"] == "record-already-green")
    assert rec.get("kind") == "system", rec
    assert any(r.get("next") == "ledger-update" for r in rec["rules"]), rec["rules"]
    # record-already-green は ledger-update の直前（台帳を緑にしてから完了判定へ渡す）
    names = [s["name"] for s in doc["steps"]]
    assert names.index("record-already-green") + 1 == names.index("ledger-update"), names
    print("9. workflow already_green →", ag_rule["next"], "(record → ledger-update)")


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


def test_already_green_reason_returned_in_output():
    """already_green の理由を stdout の reason で返し、台帳の triage.reason は変えない（AG-REASON）。"""
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
        assert result["green_recorded"] is False, result
        reason = result["reason"]
        assert isinstance(reason, str) and reason.strip(), result
        assert reason.startswith("already_green: "), reason
        node = node_entry(root)
        assert node["status"] == "unported", node
        assert node["triage"]["reason"] is None, node["triage"]
        assert set(node.keys()) == {
            "id", "name", "kind", "pekko", "status", "triage", "needs",
            "depends_on_impl", "cases", "rust", "goal",
        }, node.keys()
        print("12. already_green returns reason in stdout:", reason)


def test_existing_non_null_reason_is_preserved():
    """既存の非 null な triage.reason は already_green でも byte 不変（P4）。"""
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
        assert node["status"] == "unported", node
        print("13. existing non-null reason preserved:", node["triage"]["reason"])


def _run_mark_green(root: Path, node_id: str, total: int, passed: int, reason: str) -> bool:
    """一時台帳を作り、verify_red の mark_green をサブプロセスで呼ぶ（LEDGER は cwd 相対）。"""
    import importlib
    sys.path.insert(0, str(TOOLS))
    try:
        module = importlib.import_module("verify_red")
    finally:
        sys.path.remove(str(TOOLS))
    cwd = os.getcwd()
    try:
        os.chdir(root)
        return module.mark_green(node_id, total, passed, reason)
    finally:
        os.chdir(cwd)


def test_mark_green_persists_green_and_reason():
    """ledger-update 用の mark_green は status green・cases 実数・triage.reason を書く（green 記録）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write(root / "ledger" / "nodes.yaml", ledger_yaml([case_block(NODE_ID, None, None, None)]))
        reason = "already_green: 原典 Pekko のケース数 2 と通過ケース数 2（承認済みの除外 0 件）が一致"
        assert _run_mark_green(root, NODE_ID, 2, 2, reason) is True
        node = node_entry(root)
        assert node["status"] == "green", node
        assert node["cases"]["total"] == 2 and node["cases"]["ported"] == 2 \
            and node["cases"]["passing"] == 2, node["cases"]
        assert node["triage"]["reason"] == reason, node["triage"]
        assert set(node["triage"].keys()) == {"kind", "reason"}, node["triage"]
        print("13a. mark_green persists green + cases + reason:", node["status"])


def test_mark_green_preserves_human_reason():
    """mark_green は人間が書いた既存の triage.reason を上書きしない（False を返す）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, None, None, None)]).replace(
            "    reason: null\n",
            "    reason: 以前の理由  # do_not_port のとき必須\n",
        )
        write(root / "ledger" / "nodes.yaml", nodes_yaml)
        assert _run_mark_green(root, NODE_ID, 1, 1, "already_green: 新しい理由") is False
        node = node_entry(root)
        assert node["triage"]["reason"] == "以前の理由", node["triage"]
        assert node["status"] == "green", node
        print("13a-2. mark_green preserves human reason:", node["triage"]["reason"])


def _run_record_already_green(root: Path) -> dict:
    """record_already_green.py を cwd=root で実行し、stdout の JSON を返す。"""
    proc = subprocess.run(
        [sys.executable, str(RECORD_ALREADY_GREEN)],
        cwd=str(root), capture_output=True, text=True, check=False,
    )
    assert proc.returncode == 0, (
        f"record_already_green must always exit 0; got {proc.returncode}\n"
        f"stdout={proc.stdout}\nstderr={proc.stderr}"
    )
    return json.loads(proc.stdout)


def test_record_already_green_writes_green_and_reason():
    """result JSON が already_green のとき、台帳に status green・cases・triage.reason を書く。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write(root / "ledger" / "nodes.yaml", ledger_yaml([case_block(NODE_ID, None, None, None)]))
        write(root / ".takt" / "already-green-result.json", json.dumps({
            "classification": "already_green", "node": NODE_ID,
            "original_cases": 2, "passed": 2, "excluded": 0,
            "reason": "already_green: 原典 Pekko のケース数 2 と通過ケース数 2 が一致",
        }, ensure_ascii=False))
        result = _run_record_already_green(root)
        assert result["recorded"] is True and result["already_green"] is True, result
        node = node_entry(root)
        assert node["status"] == "green", node
        assert node["cases"]["total"] == 2 and node["cases"]["ported"] == 2 \
            and node["cases"]["passing"] == 2, node["cases"]
        assert node["triage"]["reason"].startswith("already_green: "), node["triage"]
        print("13d. record-already-green writes green + cases + reason:", node["status"])


def test_record_already_green_preserves_human_reason():
    """result JSON が already_green でも、人間が書いた既存の triage.reason は上書きしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        nodes_yaml = ledger_yaml([case_block(NODE_ID, None, None, None)]).replace(
            "    reason: null\n", "    reason: 以前の理由  # do_not_port のとき必須\n",
        )
        write(root / "ledger" / "nodes.yaml", nodes_yaml)
        write(root / ".takt" / "already-green-result.json", json.dumps({
            "classification": "already_green", "node": NODE_ID,
            "original_cases": 1, "passed": 1, "excluded": 0,
            "reason": "already_green: 新しい理由",
        }, ensure_ascii=False))
        _run_record_already_green(root)
        node = node_entry(root)
        assert node["triage"]["reason"] == "以前の理由", node["triage"]
        assert node["status"] == "green", node
        print("13e. record-already-green preserves human reason:", node["triage"]["reason"])


def test_record_already_green_skips_when_not_already_green():
    """classification が already_green でなければ台帳を書き換えない（AG-RECORD）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write(root / "ledger" / "nodes.yaml", ledger_yaml([case_block(NODE_ID, None, None, None)]))
        write(root / ".takt" / "already-green-result.json", json.dumps({
            "classification": "expected_assertion", "node": NODE_ID,
        }, ensure_ascii=False))
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = _run_record_already_green(root)
        assert result["already_green"] is False and result["recorded"] is False, result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert before == after
        print("13f. record-already-green leaves ledger untouched when not already_green")


def test_record_already_green_without_result_file():
    """result JSON が無ければ何も書かず success を返す（未実行の防御）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write(root / "ledger" / "nodes.yaml", ledger_yaml([case_block(NODE_ID, None, None, None)]))
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = _run_record_already_green(root)
        assert result["already_green"] is False, result
        assert (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8") == before
        print("13g. record-already-green with no result file is a no-op")


def test_null_reason_stays_null_when_already_green():
    """triage.reason が null のとき、already_green でも台帳には書かない（green は ledger-update）。"""
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
        assert result.get("green_recorded") is False, result
        text = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        reason_lines = [l for l in text.splitlines() if l.strip().startswith("reason:")]
        assert reason_lines == ["    reason: null"], reason_lines
        assert node_entry(root)["triage"]["reason"] is None
        print("13b. null reason stays null:", reason_lines)


def test_empty_reason_stays_empty_when_already_green():
    """triage.reason が空文字列でも、already_green は台帳を書き換えない（境界値）。"""
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
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is False, result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert before == after, (before, after)
        print("13c. empty reason untouched")



def test_already_green_does_not_add_reason_key():
    """triage.reason キーが無い台帳でも、already_green はキーを追加しない。"""
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
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result.get("green_recorded") is False, result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert before == after, (before, after)
        node = node_entry(root)
        assert "reason" not in node["triage"], node["triage"]
        assert node["status"] == "unported", node
        print("14. already_green without reason key untouched:", node["triage"])


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
    """already_green → record-already-green で書いた台帳で rollup/check_needs が壊れない。"""
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
        # verify_red は台帳を書かない。green は equivalence-review 通過後の record-already-green が書く。
        assert node_entry(root)["status"] == "unported"
        recorded = _run_record_already_green(root)
        assert recorded["recorded"] is True, recorded
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


def test_excluded_cases_count_toward_green():
    """承認済みの除外 1 件 + Rust 1 件成功で、原典 2 件が green になる。

    already_green の条件は passed + len(excluded) == original_cases であり、保存は
    total=原典件数、ported=passing=passed である。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None,
                                               excluded=["excluded case"])]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=2, with_original=True,
        )
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        result = run_verify_red(scenario)
        assert result["classification"] == "already_green", result
        assert result["excluded"] == 1, result
        assert result["original_cases"] == 2, result
        assert result["passed"] == 1, result
        assert result["green_recorded"] is False, result
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert before == after
        node = node_entry(root)
        assert node["status"] == "unported", node
        assert node["cases"]["total"] is None, node["cases"]
        assert node["cases"]["excluded"] == [
            {"name": "excluded case", "reason": "承認済みの除外",
             "approved_by": APPROVED_BY, "approved_at": datetime_date(2026, 11, 20)}
        ], node["cases"]
        print("18. excluded counted in result: cases total =", result["original_cases"],
              "excluded =", result["excluded"])


def test_excluded_without_full_pass_is_not_green():
    """除外 1 件があっても、通過数 + 除外が原典件数に満たなければ already_green にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None,
                                               excluded=["excluded case"])]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=3, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "unexpected_pass", result
        assert node_entry(root)["status"] == "unported", node_entry(root)
        print("19. excluded but unaccounted → unexpected_pass:", result["classification"])


def _excluded_node(excluded_yaml: str, node_id: str = NODE_ID) -> str:
    """case_block の除外部分だけを差し替えたノード YAML。

    excluded_yaml は `    excluded:` の値以降（空配列なら " []"、要素があれば先頭改行と
    `- ` 行）。ノードの他の欄は既存 case_block を再利用する。
    """
    node = case_block(node_id, None, None, None).replace(
        "    excluded: []\n", f"    excluded:{excluded_yaml}\n")
    assert f"    excluded:{excluded_yaml}\n" in node, node
    return node


def _count_excluded_for(excluded_yaml: str, node_id: str = NODE_ID) -> int | None:
    """1 ノードの台帳を一時ディレクトリに置き、verify_red.count_excluded を呼ぶ。"""
    return _count_excluded_for_ledger(ledger_yaml([_excluded_node(excluded_yaml, node_id)]),
                                      node_id)


def _count_excluded_for_ledger(nodes_yaml: str, node_id: str = NODE_ID) -> int | None:
    """与えた台帳 YAML を一時ディレクトリに置き、verify_red.count_excluded を呼ぶ。"""
    import importlib
    sys.path.insert(0, str(TOOLS))
    try:
        module = importlib.import_module("verify_red")
    finally:
        sys.path.remove(str(TOOLS))
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write(root / "ledger" / "nodes.yaml", nodes_yaml)
        cwd = os.getcwd()
        try:
            os.chdir(root)
            return module.count_excluded(node_id)
        finally:
            os.chdir(cwd)


def _approved_item(name: str = "a", *, by: str | None = None,
                   at: str | None = None) -> str:
    """承認記録付きの除外要素（合成値）。by/at は YAML の値表記をそのまま渡す。

    引用付きの日付・引用なしの日付・timestamp・ValueError を出す表記を試すときは、
    呼び出し側が at に YAML の表記（引用を含む）をそのまま渡す。
    """
    by = APPROVED_BY if by is None else by
    at = APPROVED_AT if at is None else at
    return (
        f"\n      - name: {name}\n"
        f"        reason: 承認済みの除外\n"
        f"        approved_by: {by}\n"
        f"        approved_at: {at}"
    )


def _unapproved_item(name: str = "excluded case") -> str:
    """承認記録を持たない除外要素（既存の name/reason だけ）。"""
    return f"\n      - name: {name}\n        reason: 承認済みの除外"


# (説明, 除外要素の YAML, 期待する count_excluded, 分類回帰を取るか)
# 分類回帰は passed + excluded == original_cases（原典2・成功1・除外1）で組み立てる。
APPROVAL_CASES = [
    ("name/reason だけ（承認記録なし）", _unapproved_item(), None, True),
    ("approved_at だけ", "\n      - name: a\n        reason: r\n"
                        f"        approved_at: {APPROVED_AT}", None, True),
    ("approved_by だけ", "\n      - name: a\n        reason: r\n"
                        f"        approved_by: {APPROVED_BY}", None, True),
    ("approved_by が空文字列", _approved_item(by='""'), None, True),
    ("approved_by が空白のみ", _approved_item(by='"  "'), None, False),
    ("approved_by が null", _approved_item(by="null"), None, False),
    ("approved_at が空文字列", _approved_item(at='""'), None, False),
    ("approved_at が null", _approved_item(at="null"), None, False),
    ("approved_at が不正な月", _approved_item(at='"2026-13-01"'), None, False),
    ("approved_at が不正な日", _approved_item(at='"2026-02-30"'), None, False),
    ("approved_at が日付でない文字列", _approved_item(at="yesterday"), None, False),
    ("approved_at が YYYYMMDD", _approved_item(at='"20261120"'), None, False),
    ("approved_at がゼロ詰めなし", _approved_item(at='"2026-1-05"'), None, False),
    ("approved_at が日時", _approved_item(at='"2026-11-20T00:00:00"'), None, False),
    ("approved_by が数値", _approved_item(by="42"), None, False),
    ("approved_by が bool", _approved_item(by="true"), None, False),
    ("approved_by がリスト", _approved_item(by="[someone]"), None, False),
    ("approved_at が bool", _approved_item(at="true"), None, False),
    ("approved_at が数値", _approved_item(at="42"), None, False),
    ("approved_at がマップ",
     _approved_item(at="{y: 2026, m: 11, d: 20}"), None, False),
    ("引用付きの承認済み", _approved_item(at='"2026-11-20"'), 1, True),
    ("引用なし（YAML 日付）の承認済み", _approved_item(at=APPROVED_AT), 1, True),
    ("全件承認済み（2 件）", _approved_item("a") + _approved_item("b"), 2, False),
    ("一部だけ未承認（1 件目だけ承認）",
     _approved_item("a") + _unapproved_item("b"), None, True),
    ("一部だけ未承認（2 件目だけ承認）",
     _unapproved_item("a") + _approved_item("b"), None, False),
    ("別要素の承認記録を流用しない",
     _approved_item("other case") + _unapproved_item(), None, False),
]


def test_excluded_approval_table():
    """承認記録の有無・妥当性の表を count_excluded で一括して確かめる（AG-APPROVED）。

    件数式が成り立つ未承認の除外があれば None、全件承認済みなら件数を返す。
    分類回帰が要る行は passed=1・原典2 で unexpected_pass を、承認済み行は already_green を確かめる。
    """
    for label, excluded_yaml, expected, classify in APPROVAL_CASES:
        actual = _count_excluded_for(excluded_yaml)
        assert actual == expected, (label, actual)
        if not classify:
            continue
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scenario = make_scenario(
                root, nodes_yaml=ledger_yaml([_excluded_node(excluded_yaml)]),
                cargo_body=CARGO_STUB_OK, passed=1,
                original_cases=2, with_original=True,
            )
            result = run_verify_red(scenario)
            want = "already_green" if expected else "unexpected_pass"
            assert result["classification"] == want, (label, result)
            assert result["excluded"] == expected, (label, result)
            assert node_entry(root)["status"] == "unported", node_entry(root)
    print(f"27. approval table → {len(APPROVAL_CASES)} cases "
          f"({sum(1 for c in APPROVAL_CASES if c[3])} with classification)")


def test_empty_excluded_is_still_counted_as_zero():
    """除外なし（空配列）は従来どおり 0 件で、承認検査を加えても Green を妨げない（AC2）。"""
    assert _count_excluded_for(" []") == 0
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=2,
            original_cases=2, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["excluded"] == 0, result
        assert result["classification"] == "already_green", result
        print("34. empty excluded → 0 / already_green:", result["classification"])


def test_approval_belongs_only_to_target_node_cases_excluded():
    """承認記録は対象ノードの cases.excluded でのみ有効で、他の場所の記録を流用しない。

    - 別ノードの承認済み除外（対象ノードは先頭と末尾の両方に置く）
    - 同一ノードの cases.excluded の外（トップレベルの excluded と cases の別キー）
    - 除外要素の下位（metadata）に置いた承認欄
    いずれも対象の除外の承認としては流用しない。
    """
    approved = _excluded_node(_approved_item("excluded case"))
    target = _excluded_node(_unapproved_item("excluded case"))
    other = approved.replace(f"- id: {NODE_ID}", f"- id: {OTHER_NODE_ID}")
    assert _count_excluded_for_ledger(ledger_yaml([other, target])) is None
    assert _count_excluded_for_ledger(ledger_yaml([target, other])) is None
    misplaced = target.replace(
        "    excluded:\n",
        "    excluded_allowed:\n"
        f"      - name: excluded case\n        approved_by: {APPROVED_BY}\n"
        f"        approved_at: {APPROVED_AT}\n"
        "    excluded:\n"
        "      - name: excluded case\n"
        "        reason: 承認済みの除外\n",
    )
    assert _count_excluded_for_ledger(ledger_yaml([misplaced])) is None
    # 同一ノードの cases の外（トップレベルの excluded）の承認済み要素は流用しない
    top_level = target.replace(
        f"  cases:\n",
        f"  excluded:\n"
        f"    - name: excluded case\n      approved_by: {APPROVED_BY}\n"
        f"      approved_at: {APPROVED_AT}\n"
        f"  cases:\n",
    )
    assert _count_excluded_for_ledger(ledger_yaml([top_level])) is None
    # 除外要素の下位（metadata）の承認欄は直下の承認として数えない
    nested = _excluded_node(
        "\n      - name: excluded case\n"
        "        reason: 承認済みの除外\n"
        "        metadata:\n"
        f"          approved_by: {APPROVED_BY}\n"
        f"          approved_at: {APPROVED_AT}"
    )
    assert _count_excluded_for_ledger(ledger_yaml([nested])) is None
    print("35. approval scoped to target node cases.excluded")


def test_unquoted_invalid_calendar_date_is_unexpected_pass():
    """引用なしの不正な暦日（YAML が ValueError を出す）でも unexpected_pass に倒す。

    PyYAML は引用なし `2026-02-30` で yaml.YAMLError ではなく ValueError を送出する。
    count_excluded はこれを握って None を返し、分類は unexpected_pass になる。
    """
    excluded_yaml = _approved_item(at="2026-02-30")
    assert _count_excluded_for(excluded_yaml) is None
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root, nodes_yaml=ledger_yaml([_excluded_node(excluded_yaml)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=2, with_original=True,
        )
        result = run_verify_red(scenario)
        assert result["classification"] == "unexpected_pass", result
        assert result["excluded"] is None, result
        assert "\n  status: unported\n" in (root / "ledger" / "nodes.yaml").read_text(
            encoding="utf-8")
    print("36. unquoted invalid date (ValueError) → unexpected_pass")


def test_excluded_timestamp_is_not_accepted_as_calendar_date():
    """時刻付き timestamp（datetime）は暦日の承認日として受理しない。"""
    excluded_yaml = _approved_item(at="2026-11-20T00:00:00Z")
    assert _count_excluded_for(excluded_yaml) is None
    excluded_yaml2 = _approved_item(at="2026-11-20 00:00:00")
    assert _count_excluded_for(excluded_yaml2) is None
    print("37. timestamp is not accepted as calendar date")


def test_non_target_invalid_date_does_not_affect_target():
    """別ノードの引用なし不正日は対象ノードの承認判定に影響しない。

    台帳全体の `yaml.safe_load` は引用なしの不正暦日（`2026-02-30`）で ValueError を
    送出する。対象ノードの選択前にこれが起きると、対象ノードの承認記録が正しくても
    count_excluded は None を返し、正常な移植が差し戻される。verify_red 専用の
    SafeLoader で timestamp スカラーを文字列として構築し、対象ノードの approved_at
    だけを暦日として検証することで、次の両方を同時に満たす:
      - 対象の不正日は fail closed（None、unexpected_pass）
      - 対象外の不正日は対象判定に影響しない（対象の承認済み件数を返す）
    共有 `yaml.SafeLoader` の状態は変更しない。
    """
    import yaml

    def assert_shared_loader_date_behavior():
        assert yaml.safe_load("approved_at: 2026-11-20")["approved_at"] == datetime_date(
            2026, 11, 20)
        try:
            yaml.safe_load("approved_at: 2026-02-30")
        except (ValueError, yaml.YAMLError):
            return
        raise AssertionError("shared SafeLoader accepted an invalid calendar date")

    target = _excluded_node(_approved_item("excluded case"))
    other_bad = target.replace(f"- id: {NODE_ID}", f"- id: {OTHER_NODE_ID}").replace(
        f"approved_at: {APPROVED_AT}", "approved_at: 2026-02-30")

    assert_shared_loader_date_behavior()
    # 対象外の不正日があっても、対象の承認済み 1 件を数える（影響しない）
    assert _count_excluded_for_ledger(ledger_yaml([other_bad, target])) == 1, "other-before"
    assert_shared_loader_date_behavior()
    assert _count_excluded_for_ledger(ledger_yaml([target, other_bad])) == 1, "other-after"
    assert_shared_loader_date_behavior()
    # 対象の不正日は fail closed
    target_bad = target.replace(f"approved_at: {APPROVED_AT}", "approved_at: 2026-02-30")
    assert _count_excluded_for_ledger(ledger_yaml([other_bad, target_bad])) is None, "target-bad"
    assert_shared_loader_date_behavior()
    print("37b. non-target invalid date does not affect target; shared loader unchanged")


def test_verify_red_imports_without_pyyaml():
    """PyYAML 非依存で verify_red を import して mark_green できる。

    HEAD のワークフローは `uv run python3 ledger/tools/record_already_green.py`
    （--with pyyaml なし）で起動し、そのスクリプトが verify_red を import する。
    count_excluded が呼ばれない限り PyYAML を要求しない（遅延 import）ことを確かめる。
    """
    code = (
        "import sys; sys.path.insert(0, %r);\n"
        "import verify_red;\n"
        "assert 'yaml' not in sys.modules, 'yaml imported eagerly';\n"
        "print('ok')\n" % str(TOOLS)
    )
    proc = subprocess.run([sys.executable, "-S", "-c", code],
                          capture_output=True, text=True, check=False)
    # -S で site-packages を外すと PyYAML は解決できない。それでも import が成功する。
    assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
    assert "ok" in proc.stdout, proc.stdout
    print("38. verify_red imports without PyYAML (lazy import)")


def test_tagged_scala_cases_are_counted():
    """taggedAs つきの Scala ケースも数え、Rust が全件通れば already_green になる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(SCALA_NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=3,
            original_cases=None, with_original=True,
            original_text=scala_original(in_cases=1, tagged_cases=2),
            node_id=SCALA_NODE_ID,
        )
        result = run_verify_red(scenario)
        assert result["original_cases"] == 3, result
        assert result["classification"] == "already_green", result
        assert node_entry(root, SCALA_NODE_ID)["status"] == "unported"
        print("20. taggedAs cases counted:", result["original_cases"])


def test_partially_countable_scala_original_is_not_green():
    """既知の書式で全件を説明できない Scala 原典（`"name" must {`）は None で、green にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(SCALA_NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=None, with_original=True,
            original_text=scala_original(in_cases=1, must_case=True),
            node_id=SCALA_NODE_ID,
        )
        result = run_verify_red(scenario)
        assert result["original_cases"] is None, result
        assert result["classification"] == "unexpected_pass", result
        assert node_entry(root, SCALA_NODE_ID)["status"] == "unported", node_entry(root, SCALA_NODE_ID)
        print("21. unaccountable Scala original → unexpected_pass:", result["classification"])


def test_mixed_supported_and_unsupported_scala_cases_not_partial():
    """対応書式と未対応書式が混在する原典で、対応書式だけの部分件数を総数にしない。

    `"ported" in {` だけを移植して Rust が 1 件通っても、未対応の `"A suite" must {`
    の下に隠れたケースを見落とさないよう total は None になり、green にしない。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = (
            "package org.apache.pekko.actor\n\n"
            "class AddressSpec extends PekkoSpec {\n"
            '  "ported" in {\n  }\n'
            '  "A suite" must {\n    "hidden" in {\n    }\n  }\n'
            "}\n"
        )
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(SCALA_NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=None, with_original=True,
            original_text=text, node_id=SCALA_NODE_ID,
        )
        result = run_verify_red(scenario)
        assert result["original_cases"] is None, result
        assert result["classification"] == "unexpected_pass", result
        assert node_entry(root, SCALA_NODE_ID)["status"] == "unported"
        print("22. mixed formats not partially counted:", result["classification"])


def test_it_should_in_cases_are_counted():
    """`it should "..." in {` 形式の原典も、名前付きブロックとして数え already_green にできる。

    SCALA_CASE は 1 件と数えるが SCALA_NAMED_PREFIX が 0 件と数えると件数不一致になり、
    この形式だけの原典は常に None になる（Green にできない）。両側で同じ行を数える。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = (
            "package org.apache.pekko.actor\n\n"
            "class AddressSpec extends PekkoSpec {\n"
            '  it should "returns host" in {\n  }\n'
            '  it should "returns port" in {\n  }\n'
            "}\n"
        )
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(SCALA_NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=2,
            original_cases=None, with_original=True,
            original_text=text, node_id=SCALA_NODE_ID,
        )
        result = run_verify_red(scenario)
        assert result["original_cases"] == 2, result
        assert result["classification"] == "already_green", result
        print("23. it should in { cases counted:", result["original_cases"])


def test_unknown_named_block_alongside_it_should_is_not_green():
    """`it should` 形式と未知の名前付きブロックが混在する原典は、件数不一致で None にする。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = (
            "package org.apache.pekko.actor\n\n"
            "class AddressSpec extends PekkoSpec {\n"
            '  it should "returns host" in {\n  }\n'
            '  "A suite" must {\n    "hidden" in {\n    }\n  }\n'
            "}\n"
        )
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(SCALA_NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_OK, passed=1,
            original_cases=None, with_original=True,
            original_text=text, node_id=SCALA_NODE_ID,
        )
        result = run_verify_red(scenario)
        assert result["original_cases"] is None, result
        assert result["classification"] == "unexpected_pass", result
        print("24. it should + unknown named block → unexpected_pass:", result["classification"])


def test_stale_result_is_removed_when_write_fails():
    """判定結果を書けないとき、前回の already_green 結果を残さず OSError を伝える。

    古い結果が残ると record-already-green がそれを読み、等価性レビューを受けていない
    green を台帳に書いてしまう。書込み失敗時は結果ファイルを削除して失敗させる。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_FAILED, passed=0,
            original_cases=1, with_original=True,
        )
        result_path = root / ".takt" / "already-green-result.json"
        write(result_path, json.dumps({
            "classification": "already_green", "node": NODE_ID,
            "original_cases": 1, "passed": 1, "excluded": 0,
            "reason": "already_green: 前回の結果",
        }, ensure_ascii=False))
        # 結果の書込みだけを失敗させる（write_result の open("w") で OSError を送出する）。
        # .takt を書込み不可にしても一部のファイルシステムでは通ってしまうため、
        # open を差し替えて書込み失敗を確実に再現する。
        import builtins
        import importlib
        sys.path.insert(0, str(TOOLS))
        try:
            verify_red_module = importlib.import_module("verify_red")
        finally:
            sys.path.remove(str(TOOLS))

        def fail_open(*args, **kwargs):
            raise OSError(28, "No space left on device (simulated)")

        real_open = builtins.open
        builtins.open = fail_open
        cwd = os.getcwd()
        try:
            os.chdir(root)
            verify_red_module.write_result({"classification": "expected_assertion"})
        except OSError as error:
            assert "simulated" in str(error), error
        else:
            raise AssertionError("write_result must propagate OSError")
        finally:
            os.chdir(cwd)
            builtins.open = real_open
        assert not result_path.exists(), result_path.read_text(encoding="utf-8")
        # 以降の実行では書込みを戻し、record-already-green が古い結果ファイルを読まずに
        # no-op（台帳を緑にしない）になることを確かめる
        record = _run_record_already_green(root)
        assert record["already_green"] is False and record["recorded"] is False, record
        assert node_entry(root)["status"] == "unported", node_entry(root)
        print("25. write failure removes stale result and record is a no-op")


def test_main_exits_nonzero_when_result_cannot_be_written():
    """判定結果を書けないとき、main の分類 JSON を返さず終了コード 1 で失敗する。

    workflow の verify_red は分類 JSON が無いと when(true) で ABORT する。0 を返して
    分類を出してしまうと、古い結果に頼った遷移が起きるため、失敗を明示する。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        scenario = make_scenario(
            root,
            nodes_yaml=ledger_yaml([case_block(NODE_ID, None, None, None)]),
            cargo_body=CARGO_STUB_FAILED, passed=0,
            original_cases=1, with_original=True,
        )
        # .takt の代わりに同名の通常ファイルを置き、makedirs/open を確実に失敗させる
        (root / ".takt").write_text("not a directory", encoding="utf-8")
        proc = subprocess.run(
            [sys.executable, str(VERIFY_RED)],
            cwd=str(root), env=scenario["env"],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        payload = json.loads(proc.stdout)
        assert "result_write_failed" in payload, payload
        print("26. write failure exits 1 without classification file")


def main() -> int:
    tests = [
        test_already_green_when_ported_and_satisfied,
        test_already_green_writes_result_file,
        test_non_already_green_result_file_has_no_reason,
        test_unexpected_pass_when_original_missing,
        test_unexpected_pass_when_case_count_mismatch,
        test_unexpected_pass_when_zero_cases_pass,
        test_expected_compile_error_marks_red,
        test_expected_assertion_marks_red,
        test_mark_red_only_touches_status_line,
        test_already_green_leaves_ledger_untouched,
        test_workflow_has_already_green_branch_to_verify_green,
        test_loop_monitors_unchanged,
        test_already_green_reason_returned_in_output,
        test_existing_non_null_reason_is_preserved,
        test_mark_green_persists_green_and_reason,
        test_mark_green_preserves_human_reason,
        test_record_already_green_writes_green_and_reason,
        test_record_already_green_preserves_human_reason,
        test_record_already_green_skips_when_not_already_green,
        test_record_already_green_without_result_file,
        test_null_reason_stays_null_when_already_green,
        test_empty_reason_stays_empty_when_already_green,
        test_already_green_does_not_add_reason_key,
        test_parse_passed_skips_leading_empty_summary,
        test_parse_passed_sums_multiple_summaries,
        test_parse_passed_fails_when_last_summary_failed,
        test_rollup_and_check_needs_do_not_break,
        test_excluded_cases_count_toward_green,
        test_excluded_without_full_pass_is_not_green,
        test_excluded_approval_table,
        test_empty_excluded_is_still_counted_as_zero,
        test_approval_belongs_only_to_target_node_cases_excluded,
        test_unquoted_invalid_calendar_date_is_unexpected_pass,
        test_excluded_timestamp_is_not_accepted_as_calendar_date,
        test_non_target_invalid_date_does_not_affect_target,
        test_verify_red_imports_without_pyyaml,
        test_tagged_scala_cases_are_counted,
        test_partially_countable_scala_original_is_not_green,
        test_mixed_supported_and_unsupported_scala_cases_not_partial,
        test_it_should_in_cases_are_counted,
        test_unknown_named_block_alongside_it_should_is_not_green,
        test_stale_result_is_removed_when_write_fails,
        test_main_exits_nonzero_when_result_cannot_be_written,
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
