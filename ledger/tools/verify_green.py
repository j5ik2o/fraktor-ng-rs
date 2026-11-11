#!/usr/bin/env python3
"""verify-green の機械ゲート（TAKT の system step から固定コマンドで呼ばれる）。

直前の implement レポート（環境変数 TAKT_PREVIOUS_RESPONSE_FILE）から
対象クレートを読み、機械ゲートをすべて実行して結果を JSON で出す。
終了コードは常に 0 で、合否は JSON の all_pass が持つ。

ゲート:
  test     cargo test -p <crate>（クレート全体。既存テストの退行検出を含む）
  clippy   cargo clippy -p <crate> -- -D warnings
  fmt      cargo fmt --check
  syn_checker / aislop は、実行物が導入されたらここに追加する。
  現時点では "not_installed" として報告し、合否には含めない。
"""
import json
import os
import re
import subprocess


def parse_crate() -> str | None:
    # 1. 直前レスポンスの `crate:` 行（あれば最優先）
    path = os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", "")
    if path and os.path.exists(path):
        text = open(path, encoding="utf-8").read()
        m = re.search(r"^crate:\s*(\S+)\s*$", text, re.MULTILINE)
        if m:
            return m.group(1)
    # 2. 台帳で status が red のノード（処理中の1件）の rust.crate
    ledger = "ledger/nodes.yaml"
    if os.path.exists(ledger):
        try:
            import yaml
            data = yaml.safe_load(open(ledger, encoding="utf-8"))
            for node in data.get("nodes", []):
                if node.get("status") == "red":
                    crate = (node.get("rust") or {}).get("crate")
                    if crate:
                        return crate
        except Exception:
            pass
    return None


def run(cmd: list[str], timeout: int = 840) -> dict:
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return {"result": "pass" if proc.returncode == 0 else "fail",
                "detail": (proc.stdout + proc.stderr)[-800:]}
    except Exception as e:  # 起動できない・タイムアウト等は fail として報告する
        return {"result": "fail", "detail": f"{type(e).__name__}: {e}"}


def main() -> int:
    crate = parse_crate()
    if not crate:
        print(json.dumps({"all_pass": False,
                          "detail": "previous response lacks a crate line"}))
        return 0

    gates = {
        "test": run(["cargo", "test", "-p", crate]),
        "clippy": run(["cargo", "clippy", "-p", crate, "--", "-D", "warnings"]),
        "fmt": run(["cargo", "fmt", "--check"]),
        "syn_checker": {"result": "not_installed", "detail": ""},
        "aislop": {"result": "not_installed", "detail": ""},
    }
    all_pass = all(g["result"] in ("pass", "not_installed") for g in gates.values())
    print(json.dumps({"all_pass": all_pass, "crate": crate, "gates": gates},
                     ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
