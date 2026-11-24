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

成功: {"recorded": true, "already_green": true, "node": "...", "status": "green"}
対象外: {"recorded": false, "already_green": false, "classification": "..."}
失敗: {"recorded": false, "already_green": true, "detail": "..."}
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import pekko_baseline  # noqa: E402  (照合済みタグは用意された原典の HEAD から決める)
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


def main() -> int:
    result = load_result(verify_red.ALREADY_GREEN_RESULT)
    classification = result.get("classification")
    if classification != "already_green":
        print(json.dumps({"recorded": False, "already_green": False,
                          "classification": classification}, ensure_ascii=False))
        return 0

    node = result.get("node")
    total = result.get("original_cases")
    passed = result.get("passed")
    reason = result.get("reason")
    if not (isinstance(node, str) and isinstance(total, int)
            and isinstance(passed, int) and isinstance(reason, str)):
        print(json.dumps({"recorded": False, "already_green": True,
                          "detail": "already-green-result.json lacks node/cases/reason"},
                         ensure_ascii=False))
        return 0

    # 照合に使った原典が現行の基準でなければ green にしない（旧原典の照合に現行タグを付けない）。
    try:
        tag = pekko_baseline.source_tag(
            pekko_baseline.load(),
            os.environ.get("PEKKO_ORIGINAL_ROOT", verify_red.DEFAULT_ORIGINAL_ROOT),
        )
    except pekko_baseline.BaselineError as error:
        print(json.dumps({"recorded": False, "already_green": True, "node": node,
                          "detail": f"the Pekko source is not the current baseline: {error}"},
                         ensure_ascii=False))
        return 0

    ok = verify_red.mark_green(node, total, passed, reason, tag)
    if not ok:
        print(json.dumps({"recorded": False, "already_green": True, "node": node,
                          "detail": "mark_green could not rewrite the ledger node"},
                         ensure_ascii=False))
        return 0
    print(json.dumps({"recorded": True, "already_green": True, "node": node,
                      "status": "green"}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
