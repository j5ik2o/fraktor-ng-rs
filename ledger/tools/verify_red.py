#!/usr/bin/env python3
"""verify-red の機械判定（TAKT の system step から固定コマンドで呼ばれる）。

直前の port-test レポート（環境変数 TAKT_PREVIOUS_RESPONSE_FILE）から
対象ノード・クレート・テストファイルを読み、対象テストだけを実行して
失敗理由を分類する。想定どおりの失敗なら台帳の status を red に更新する。

分類（stdout に JSON で出す。終了コードは常に 0 で、成否は JSON が持つ）:
  expected_compile_error  未実装によるコンパイルエラー（想定どおり）
  expected_assertion      テストが実行され、アサーションで落ちた（想定どおり）
  unexpected_pass         最初から通ってしまった
  unclassifiable          機械分類できない（軽量モデルの判定ステップへ）

台帳の書き換えは外部ライブラリに頼らず、対象ノードのブロック内の
status 行だけを置き換える（nodes.yaml は変換器の生成形式に依存する）。
"""
import json
import os
import re
import subprocess
import sys

LEDGER = "ledger/nodes.yaml"


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
    if not os.path.exists(LEDGER):
        return False
    lines = open(LEDGER, encoding="utf-8").read().splitlines(keepends=True)
    in_block = False
    for i, line in enumerate(lines):
        if line.startswith("- id: "):
            in_block = line.split(":", 1)[1].strip() == node_id
            continue
        if in_block and re.match(r"\s+status:\s*", line):
            indent = line[: len(line) - len(line.lstrip())]
            lines[i] = f"{indent}status: red\n"
            open(LEDGER, "w", encoding="utf-8").write("".join(lines))
            return True
    return False


def main() -> int:
    info = parse_previous_response()
    node = info.get("node")
    crate = info.get("crate")
    test_file = info.get("test_file")
    if not (node and crate and test_file):
        print(json.dumps({"classification": "unclassifiable",
                          "detail": "previous response lacks node/crate/test_file lines"}))
        return 0

    stem = os.path.splitext(os.path.basename(test_file))[0]
    proc = subprocess.run(
        ["cargo", "test", "-p", crate, "--test", stem],
        capture_output=True, text=True, timeout=540,
    )
    out = proc.stdout + proc.stderr

    if "test result: ok" in out:
        classification = "unexpected_pass"
    elif "test result: FAILED" in out:
        classification = "expected_assertion"
    elif re.search(r"^error(\[|:)", out, re.MULTILINE):
        classification = "expected_compile_error"
    else:
        classification = "unclassifiable"

    red_recorded = False
    if classification.startswith("expected_"):
        red_recorded = mark_red(node)

    print(json.dumps({
        "classification": classification,
        "node": node,
        "red_recorded": red_recorded,
        "detail": out[-1500:],
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
