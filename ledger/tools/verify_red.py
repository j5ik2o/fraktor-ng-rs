#!/usr/bin/env python3
"""verify-red の機械判定（TAKT の system step から固定コマンドで呼ばれる）。

直前の port-test レポート（環境変数 TAKT_PREVIOUS_RESPONSE_FILE）から
対象ノード・クレート・テストファイルを読み、対象テストだけを実行して
失敗理由を分類する。想定どおりの失敗なら台帳の status を red に更新する。

分類（stdout に JSON で出す。終了コードは常に 0 で、成否は JSON が持つ）:
  expected_compile_error  未実装によるコンパイルエラー（想定どおり）
  expected_assertion      テストが実行され、アサーションで落ちた（想定どおり）
  already_green           移植が正しく、前提の実装が既に契約を満たしている
  unexpected_pass         最初から通ったが、移植の正しさを確かめられない
  unclassifiable          機械分類できない（軽量モデルの判定ステップへ）
  source_rejected         原典が変更のない現行の基準 commit でない（cargo も台帳も触らず止める）

already_green は「移植の正しさ（原典 Pekko のケース数と、人間が承認した除外の件数の和が、
通過したケース数に一致し、1 件以上通っている）」と「前提の実装が契約を満たす（対象テストの
全件成功）」の両方が確かめられる場合に限る。片方でも欠ける場合は unexpected_pass に倒す。
原典が用意されていない環境（pekko/ が無い）、または原典のケースをすべて数えたと確認できない
ファイル（WordSpec の `"name" must {` など既知の書式で全件を説明できない構文を含む）では
移植を確かめられないため already_green にはしない。

台帳の書き換えは外部ライブラリに頼らず、対象ノードのブロック内の
status 行だけを置き換える（nodes.yaml は変換器の生成形式に依存する）。
already_green のときは台帳を書き換えない。classification と件数（original_cases / passed /
excluded）と理由（already_green: ...）を返すだけにし、status を green に永続化するのは
verify_green と equivalence-review を通過した後の record-already-green（record_already_green.py）
に任せる。triage は green を再投入しないため、この段階で green を書くと、等価性レビューが
中断したときに未承認の green が残ってしまう。workflow は already_green を verify_green へ進める。

already_green の判定結果（node / original_cases / passed / excluded / reason）は
`.takt/already-green-result.json` へ書く。system step の次段へは台帳へ書く前に件数を渡せず、
TAKT の system step 間の応答も JSON ではないため、record-already-green がこの固定パスを読む。
already_green でないときは classification だけを持つ JSON を書く（record は何もしない）。
同じパスには前回の実行の結果が残りうるため、判定結果は常にこの実行の内容で上書きし、
書込みに失敗したときは既存の結果ファイルを削除して終了コード 1 で失敗する（古い
already_green を record-already-green に読ませない。workflow の verify_red は分類 JSON を
返さない終了コード 1 を when(true) で ABORT し、古い結果のまま台帳へ進ませない）。
"""
import json
import os
import re
import subprocess
import sys

import excluded_approvals
from ledger_lines import (  # noqa: F401  (台帳の行単位の書き換え。以前からの名前を残す)
    _insert_after_key, _is_empty_reason, _leaves_nodes, _set_top_level_key, _yaml_scalar)
import pekko_baseline

LEDGER = "ledger/nodes.yaml"

# already_green の判定結果を record-already-green へ渡す固定パス（cwd 相対）。
ALREADY_GREEN_RESULT = os.path.join(".takt", "already-green-result.json")

# 原典 Pekko の場所は pekko_baseline.source_root()（PEKKO_ORIGINAL_ROOT、無ければ cwd の pekko/。
# tools/pekko-source/prepare_pekko_source.sh が用意する）。record-already-green と ledger-update も
# 同じ関数で場所を決め、同じ検査（変更のない現行の基準 commit か）を通す。

# 原典のケース書式。Java は @Test 注釈と、その直後の void メソッド宣言。
JAVA_TEST = re.compile(r"^\s*@Test\b", re.MULTILINE)
JAVA_TEST_DECL = re.compile(
    r"^\s*@Test\b[^\n]*\n(?:\s*@[^\n]*\n)*\s*(?:public|protected|private)?\s*"
    r"(?:static\s+)?(?:final\s+)?void\s+\w+\s*\(",
    re.MULTILINE,
)
# Scala のケース書式。`"..." in {` の前に taggedAs が付く形も同じケースである。
# `it should "..."` の書式も数える。`in {` に至る書式を 1 つの正規表現にまとめる。
SCALA_CASE = re.compile(
    r"""^\s*(?:
        it\s+should\s+"[^"]*"\s+in\s*\{
      | "[^"]*"(?:\s+taggedAs\b[^\n{]*?)?\s+in\s*\{
    )""",
    re.MULTILINE | re.VERBOSE,
)
# 既知の書式で説明できない名前付きブロック（`"name" must {`、`"name" should {` など）。
# `it should "..."` で始まる行も、文字列リテラルで始まる名前付きブロックとして数える
# （SCALA_CASE 側でケースとして数えるため、突き合わせの両側で同じ行を数える）。
SCALA_NAMED_PREFIX = re.compile(r'^\s*(?:it\s+should\s+)?"[^"]+"\s+(?=\S)', re.MULTILINE)


def parse_previous_response() -> dict:
    path = os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", "")
    info = {}
    if path and os.path.exists(path):
        text = open(path, encoding="utf-8").read()
        for key in ("node", "crate", "test_file"):
            m = re.search(rf"^{key}:\s*(\S+)\s*$", text, re.MULTILINE)
            if m:
                info[key] = m.group(1)
    return info


def mark_red(node_id: str) -> bool:
    """台帳の対象ノードの status を red にする。成功したら True。"""
    return _rewrite_node(node_id, lambda i, line: _replace_status(line, "red"))


def mark_green(node_id: str, total: int, passed: int, reason: str, verified_against: str) -> bool:
    """台帳の対象ノードを green にし、cases の実数・triage.reason・verified_against を書く。成功したら True。

    verified_against には照合に使った原典の基準タグを渡す（呼び出し側が
    pekko_baseline.source_tag で、用意された pekko/ の HEAD から決める）。空なら何も書かない。

    verify_green と equivalence-review を通過した後、record-already-green（record_already_green.py）
    から呼ぶ。verify_red の already_green 分類では呼ばない（等価性レビュー中断時に未承認の
    green を残さないため）。

    total には原典のケース数、ported と passing には Rust の成功件数 passed を書く
    （承認済みの除外がある場合、total = passed + len(excluded) になる）。
    status・cases の total/ported/passing・既存の triage.reason・最上位の verified_against だけを
    書き換える。verified_against 行が無ければノードの末尾に足し、それ以外の新キーは増やさない。
    triage.reason が null または空のときだけ `already_green: ` 理由を書き、人間が書いた既存の
    理由は上書きしない。triage.reason キーが無い台帳でも green と cases は書き、理由は載せない。
    """
    if not os.path.exists(LEDGER):
        return False
    lines = open(LEDGER, encoding="utf-8").read().splitlines(keepends=True)
    in_block = False
    in_cases = False
    in_triage = False
    status_written = False
    cases_written = False
    reason_written = False
    reason_preserved = False
    reason_indent = "    "
    for i, line in enumerate(lines):
        if _leaves_nodes(line):
            break
        if line.startswith("- id: "):
            in_block = line.split(":", 1)[1].strip() == node_id
            in_cases = False
            in_triage = False
            continue
        if not in_block:
            continue
        base_indent = len(line) - len(line.lstrip())
        if re.match(r"\s+cases:\s*$", line):
            in_cases = True
            in_triage = False
            continue
        if re.match(r"\s+triage:\s*$", line):
            in_triage = True
            in_cases = False
            reason_indent = line[: len(line) - len(line.lstrip())] + "  "
            continue
        if base_indent <= 2:
            in_cases = False
            in_triage = False
        if re.match(r"\s+status:\s*", line):
            lines[i] = _replace_status(line, "green")
            status_written = True
        elif in_cases:
            m = re.match(r"(\s+)(total|ported|passing):", line)
            if m:
                value = total if m.group(2) == "total" else passed
                lines[i] = f"{m.group(1)}{m.group(2)}: {value}\n"
                cases_written = True
        elif in_triage and re.match(r"\s+reason:", line):
            reason_written = True
            if _is_empty_reason(line):
                lines[i] = f"{reason_indent}reason: {_yaml_scalar(reason)}\n"
            else:
                reason_preserved = True
    if not verified_against or not (status_written and cases_written):
        return False
    if not reason_written:
        reason_line = f"{reason_indent}reason: {_yaml_scalar(reason)}\n"
        lines = _insert_after_key(lines, node_id, "triage:", reason_line)
    lines = _set_top_level_key(lines, node_id, "verified_against", verified_against)
    open(LEDGER, "w", encoding="utf-8").write("".join(lines))
    return not reason_preserved


def _rewrite_node(node_id: str, transform) -> bool:
    """対象ノードのブロックで最初に transform が True を返した行を置換して書き戻す。"""
    if not os.path.exists(LEDGER):
        return False
    lines = open(LEDGER, encoding="utf-8").read().splitlines(keepends=True)
    in_block = False
    for i, line in enumerate(lines):
        if _leaves_nodes(line):
            break
        if line.startswith("- id: "):
            in_block = line.split(":", 1)[1].strip() == node_id
            continue
        if in_block and re.match(r"\s+status:\s*", line):
            lines[i] = transform(i, line)
            open(LEDGER, "w", encoding="utf-8").write("".join(lines))
            return True
    return False


def _replace_status(line: str, status: str) -> str:
    indent = line[: len(line) - len(line.lstrip())]
    return f"{indent}status: {status}\n"


def count_original_cases(node_id: str) -> int | None:
    """原典 Pekko テストのケース数を数える。原典が無い・全件を数えたと確認できないなら None。

    Java の @Test と、Scala の `it should "..." in {` / `"..." in {` / `"..." taggedAs ... in {`
    を数える。既知の書式で全件を説明できるかを、ファイル全体の構文本数と突き合わせて確かめる:
    Java は @Test の数と、@Test に続く void メソッド宣言の数が一致すること。
    Scala は先頭が文字列リテラルで始まる行の数が、上の書式で数えたケース数に一致すること
    （`"name" must {`、`"name" should {` などはケース書式ではないため、そのファイルは
    数えられないと判断する）。
    0 件は「ケースが無い」であり、移植の正しさを確かめられないため None とする。
    """
    path = os.path.join(pekko_baseline.source_root(), node_id)
    if not os.path.exists(path):
        return None
    try:
        text = open(path, encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        return None
    if path.endswith(".java"):
        java = len(JAVA_TEST.findall(text))
        java_decl = len(JAVA_TEST_DECL.findall(text))
        if java != java_decl:
            return None
        total = java
    elif path.endswith(".scala"):
        scala_cases = len(SCALA_CASE.findall(text))
        named = len(SCALA_NAMED_PREFIX.findall(text))
        if named != scala_cases:
            return None
        total = scala_cases
    else:
        return None
    return total if total > 0 else None


def count_excluded(node_id: str) -> int | None:
    """台帳の対象ノードの cases.excluded のうち、人間が承認した件数を返す。

    読み込み（厳格な SafeLoader）、承認記録の判定、件数計算は同ディレクトリの
    `excluded_approvals` が持つ。ここでは呼出し時点の `LEDGER`（cwd 相対）を渡すだけにし、
    台帳パスをモジュール読込時に固定しない。未承認・不正な承認記録が 1 件でもあれば
    None を返し、already_green を許可しない（詳細は `excluded_approvals.count_excluded`）。
    """
    return excluded_approvals.count_excluded(LEDGER, node_id)


def parse_passed(out: str) -> int | None:
    """対象テストの通過ケース数を返す。失敗の要約を含む・0 件・要約が無い場合は None。

    実 cargo は先頭に 0 件の test result（別バイナリの要約）を出し、続けて対象テストの
    要約を出すことがある。先頭行だけを採ると対象テストの結果を取りこぼすため、
    全文の要約をすべて合算し、1 行でも FAILED を含むなら None とする。
    """
    results = re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed;",
                         out, re.MULTILINE)
    if not results:
        return None
    passed = 0
    for status, p, f in results:
        if status != "ok" or int(f) != 0:
            return None
        passed += int(p)
    return passed if passed > 0 else None


def write_result(result: dict) -> None:
    """record-already-green が読む固定パスへ判定結果を書く。前回の結果は必ず置き換える。

    前回の実行の already_green 結果が残ったまま record-already-green に読まれると、等価性
    レビューを受けていない green が台帳に残る。そのため書く前に古い結果ファイルを消し、
    書込みに失敗したときも古い結果を残さず OSError を呼出し側へ伝える。
    """
    os.makedirs(os.path.dirname(ALREADY_GREEN_RESULT), exist_ok=True)
    _remove_stale_result()
    with open(ALREADY_GREEN_RESULT, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False)


def _remove_stale_result() -> None:
    """古い判定結果を残さない。結果ファイルが無いときは何もしない。"""
    try:
        os.remove(ALREADY_GREEN_RESULT)
    except FileNotFoundError:
        return


def main() -> int:
    info = parse_previous_response()
    node = info.get("node")
    crate = info.get("crate")
    test_file = info.get("test_file")

    # 原典が変更のない現行の基準でなければ、ケース数も分類も信用できない。台帳を書かずに止める
    # （workflow は source_rejected を when(true) の ABORT で受ける）。原典には触れない。
    root = pekko_baseline.source_root()
    try:
        baseline = pekko_baseline.load()
        tag = pekko_baseline.source_tag(baseline, root)
    except pekko_baseline.BaselineError as error:
        result = {"classification": "source_rejected", "node": node, "detail": str(error)}
        try:
            write_result(result)
        except OSError as write_error:
            print(json.dumps({**result, "result_write_failed": str(write_error)},
                             ensure_ascii=False))
            return 1
        print(json.dumps(result, ensure_ascii=False))
        return 0
    # 出所。record-already-green はこの 3 つが揃い、記録時の原典と一致する結果だけを受け付ける
    # （出所の無い古い結果や、別の原典で出した結果で green を書かない）。
    provenance = {"source_tag": tag, "source_commit": baseline.commit,
                  "source_root": str(root.resolve())}

    if not (node and crate and test_file):
        result = {"classification": "unclassifiable",
                  "detail": "previous response lacks node/crate/test_file lines", **provenance}
        try:
            write_result(result)
        except OSError as error:
            print(json.dumps({**result, "result_write_failed": str(error)},
                             ensure_ascii=False))
            return 1
        print(json.dumps(result, ensure_ascii=False))
        return 0

    stem = os.path.splitext(os.path.basename(test_file))[0]
    proc = subprocess.run(
        ["cargo", "test", "-p", crate, "--test", stem],
        capture_output=True, text=True, timeout=540,
    )
    out = proc.stdout + proc.stderr

    original_cases = count_original_cases(node)
    passed = parse_passed(out)
    excluded = count_excluded(node)

    already_green = (
        passed is not None and original_cases is not None and excluded is not None
        and passed + excluded == original_cases
    )
    if already_green:
        classification = "already_green"
    elif "test result: ok" in out:
        classification = "unexpected_pass"
    elif "test result: FAILED" in out:
        classification = "expected_assertion"
    elif re.search(r"^error(\[|:)", out, re.MULTILINE):
        classification = "expected_compile_error"
    else:
        classification = "unclassifiable"

    red_recorded = False
    green_recorded = False
    reason = None
    if classification.startswith("expected_"):
        red_recorded = mark_red(node)
    elif classification == "already_green":
        reason = (
            f"already_green: 原典 Pekko のケース数 {original_cases} と通過ケース数 "
            f"{passed}（承認済みの除外 {excluded} 件）が一致し、対象テストが全件成功している"
            f"（前提の実装が契約を満たす）"
        )

    result = {
        "classification": classification,
        "node": node,
        "red_recorded": red_recorded,
        "green_recorded": green_recorded,
        "original_cases": original_cases,
        "passed": passed,
        "excluded": excluded,
        "reason": reason,
        **provenance,
        "detail": out[-1500:],
    }
    try:
        write_result(result)
    except OSError as error:
        print(json.dumps({**result, "result_write_failed": str(error)},
                         ensure_ascii=False))
        return 1
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
