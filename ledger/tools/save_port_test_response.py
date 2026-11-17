#!/usr/bin/env python3
"""port-test の応答 3 行を `.takt/port-test-response.txt` へ保存する（TAKT の system step から呼ばれる）。

`port-test` は agent step で、作業フェーズの応答に `node`／`crate`／`test_file`
の 3 行を定める（`.takt/facets/instructions/fraktor-port-test.md`）。その応答は
環境変数 TAKT_PREVIOUS_RESPONSE_FILE のファイルとして渡る。`verify_green` は
`--response .takt/port-test-response.txt` を読むため、この工程が 3 行を転記する。
指示文への書き出し依頼だけでは、workflow の実行で生成を保証できない。

3 行が揃わない場合は保存せずに失敗する。古い内容を残すと、`verify_green` が
別のときの応答を今回の応答として検証し得るため、書き出し前に既存の保存先を消す。

stdout に JSON で結果を出す。終了コードは常に 0 で、成否は `saved` が持つ
（workflow の遷移は JSON を読む）。

成功: {"saved": true, "path": "...", "node": "...", "crate": "...", "test_file": "..."}
失敗: {"saved": false, "detail": "..."}
"""
import json
import os
import re
import sys

RESPONSE_KEYS = ("node", "crate", "test_file")
TARGET = os.path.join(".takt", "port-test-response.txt")


def read_response(path: str) -> dict:
    """応答ファイルから 3 行を取り出す。読めない場合は空の辞書を返す。"""
    if not path or not os.path.exists(path):
        return {}
    text = open(path, encoding="utf-8").read()
    info = {}
    for key in RESPONSE_KEYS:
        m = re.search(rf"^{key}:\s*(\S+)\s*$", text, re.MULTILINE)
        if m:
            info[key] = m.group(1)
    return info


def discard_target() -> None:
    """前回の保存内容を消す。"""
    if os.path.exists(TARGET):
        os.remove(TARGET)


def fail(detail: str) -> int:
    discard_target()
    print(json.dumps({"saved": False, "detail": detail}, ensure_ascii=False))
    return 0


def main() -> int:
    source = os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", "")
    if not source:
        return fail("TAKT_PREVIOUS_RESPONSE_FILE is not set")
    if not os.path.exists(source):
        return fail(f"previous response file not found: {source}")
    info = read_response(source)
    missing = [key for key in RESPONSE_KEYS if key not in info]
    if missing:
        return fail(f"previous response lacks {', '.join(missing)} lines: {source}")

    os.makedirs(os.path.dirname(TARGET), exist_ok=True)
    with open(TARGET, "w", encoding="utf-8") as out:
        for key in RESPONSE_KEYS:
            out.write(f"{key}: {info[key]}\n")
    print(json.dumps({"saved": True, "path": TARGET, **{key: info[key] for key in RESPONSE_KEYS}},
                     ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
