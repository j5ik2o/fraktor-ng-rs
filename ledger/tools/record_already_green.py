#!/usr/bin/env python3
"""already_green の判定結果を台帳へ機械的に記録する（TAKT の system step から呼ばれる）。

verify_red は already_green のとき台帳を書き換えず、判定結果（node / original_cases /
passed / excluded / reason）を `.takt/already-green-result.json` へ書く。本スクリプトは
その JSON を読み、classification が already_green のときだけ、対象ノードに
status: green・cases の実数・triage.reason（already_green: ...）と、照合に使った原典の基準タグ
（verified_against。pekko/ の HEAD が現行の基準 commit のときだけ）を書く。それ以外の
classification では台帳を変えずに success を返す（次は ledger-update へ進む）。

台帳の書き換えは verify_red の mark_green をそのまま使う。status 行・cases の
total/ported/passing・既存の triage.reason・最上位の verified_against だけを書き換える
（verified_against 行が無ければノードの末尾に足す。それ以外の新しいキーは増やさない）。
人間が書いた既存の triage.reason は上書きしない。

placement: equivalence-review を通過した後、ledger-update の前。already_green は
verify_green → equivalence-review を通過してから green を永続化するため、等価性レビューが
中断した回に未承認の green を残さない。triage は green を再投入しない設計とも両立する。

stdout に JSON で結果を出す。終了コードは常に 0 で、成否は `recorded` が持つ
（workflow の遷移は JSON を読む）。

成功: {"recorded": true, "already_green": true, "source_ok": true, "node": "...", "status": "green", ...}
対象外: {"recorded": false, "already_green": false, "source_ok": true, "classification": "..."}
失敗: {"recorded": false, "already_green": true, "source_ok": true, "detail": "..."}
原典の拒否: {"recorded": false, "source_ok": false, "detail": "..."}

原典（pekko_baseline.source_root）は classification によらず毎回検査する。変更のない現行の
基準 commit でないか、verify_red の結果が無い・出所（source_tag / source_commit /
source_root）を欠く・出所が今の原典と一致しないときは source_ok: false を返し、
workflow はこのとき ledger-update へ進まず ABORT する（ledger-update が green を書かない）。
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import pekko_baseline  # noqa: E402  (原典の場所と照合済みタグは verify_red と同じ規則で決める)
import verify_red  # noqa: E402  (台帳の書き換え規則を 1 箇所に保つ)


def load_result(path: str) -> dict:
    """判定結果の JSON を読む。無い・読めない場合は空の辞書。"""
    if not os.path.exists(path):
        return {}
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


PROVENANCE = ("source_tag", "source_commit", "source_root")


def check_source(result: dict) -> tuple[str | None, str | None]:
    """原典と verify_red の結果の出所を検査し、(基準タグ, 拒否の理由) を返す。

    verify_red と同じ場所（pekko_baseline.source_root）を同じ検査に通す。さらに、結果が
    出所（source_tag / source_commit / source_root）を持ち、それが今の原典の場所・現行の
    基準のタグと commit に一致することを求める。結果が無い、出所が欠けている・不正、
    または一致しないときは拒否する（出所の無い旧形式の結果を、同じ場所に置き直した
    現行の原典のタグで記録しない）。拒否の理由は verify_red のやり直しを促す。
    """
    rerun = "re-run verify_red against the prepared source"
    root = pekko_baseline.source_root()
    try:
        baseline = pekko_baseline.load()
        tag = pekko_baseline.source_tag(baseline, root)
    except pekko_baseline.BaselineError as error:
        return None, f"the Pekko source is not an unmodified current baseline: {error}"
    if not result:
        return None, f"no verify_red result at {verify_red.ALREADY_GREEN_RESULT}; {rerun}"
    bad = [k for k in PROVENANCE if not isinstance(result.get(k), str) or not result.get(k)]
    if bad:
        return None, f"verify_red result lacks provenance {bad}; {rerun}"
    if os.path.realpath(result["source_root"]) != os.path.realpath(root):
        return None, f"verify_red used {result['source_root']}, but this step resolves {root.resolve()}; {rerun}"
    if (result["source_tag"], result["source_commit"]) != (tag, baseline.commit):
        return None, (f"verify_red verified against {result['source_tag']} {result['source_commit']}, "
                      f"but the current baseline is {tag} {baseline.commit}; {rerun}")
    return tag, None


def main() -> int:
    result = load_result(verify_red.ALREADY_GREEN_RESULT)
    classification = result.get("classification")
    already_green = classification == "already_green"
    # 原典の検査は classification によらず行う。拒否されたら workflow は ledger-update へ進まない。
    tag, rejected = check_source(result)
    if rejected:
        print(json.dumps({"recorded": False, "already_green": already_green, "source_ok": False,
                          "node": result.get("node"), "detail": rejected}, ensure_ascii=False))
        return 0
    if not already_green:
        print(json.dumps({"recorded": False, "already_green": False, "source_ok": True,
                          "classification": classification}, ensure_ascii=False))
        return 0

    node = result.get("node")
    total = result.get("original_cases")
    passed = result.get("passed")
    reason = result.get("reason")
    if not (isinstance(node, str) and isinstance(total, int)
            and isinstance(passed, int) and isinstance(reason, str)):
        print(json.dumps({"recorded": False, "already_green": True, "source_ok": True,
                          "detail": "already-green-result.json lacks node/cases/reason"},
                         ensure_ascii=False))
        return 0

    ok = verify_red.mark_green(node, total, passed, reason, tag)
    if not ok:
        print(json.dumps({"recorded": False, "already_green": True, "source_ok": True, "node": node,
                          "detail": "mark_green could not rewrite the ledger node"},
                         ensure_ascii=False))
        return 0
    print(json.dumps({"recorded": True, "already_green": True, "source_ok": True, "node": node,
                      "status": "green", "verified_against": tag}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
