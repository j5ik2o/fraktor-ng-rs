#!/usr/bin/env python3
"""verify-green の機械ゲート（TAKT の system step から固定コマンドで呼ばれる）。

直前の implement レポート（環境変数 TAKT_PREVIOUS_RESPONSE_FILE）から
対象クレートを読み、機械ゲートをすべて実行して結果を JSON で出す。
終了コードは常に 0 で、合否は JSON の all_pass が持つ。

ゲート:
  test     cargo test -p <crate>（クレート全体。既存テストの退行検出を含む）
  clippy   cargo clippy -p <crate> -- -D warnings
  fmt      cargo fmt --check
  syn_checker  tools/syn-check を対象クレートのディレクトリに対して実行する。
               違反かパース不能のファイルがあれば fail。チェッカーが無い環境
               （tools/syn-check が無い、cargo が無い）では "not_installed" として
               報告し、合否には含めない。
  aislop は、実行物が導入されたらここに追加する。
  現時点では "not_installed" として報告し、合否には含めない。
"""
import json
import os
import re
import shutil
import subprocess
from pathlib import Path

SYN_CHECK_MANIFEST = Path(__file__).resolve().parents[2] / "tools" / "syn-check" / "Cargo.toml"


def parse_crate() -> tuple[str | None, list[str]]:
    """対象クレート名と、特定できなかった場合の理由の一覧を返す。"""
    notes: list[str] = []
    # 1. 直前レスポンスの `crate:` 行（あれば最優先）
    path = os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", "")
    if not path:
        notes.append("TAKT_PREVIOUS_RESPONSE_FILE is not set")
    elif not os.path.exists(path):
        notes.append(f"previous response file not found: {path}")
    else:
        text = open(path, encoding="utf-8").read()
        m = re.search(r"^crate:\s*(\S+)\s*$", text, re.MULTILINE)
        if m:
            return m.group(1), notes
        notes.append("previous response lacks a crate line")
    # 2. 台帳で status が red のノード（処理中の1件）の rust.crate
    ledger = "ledger/nodes.yaml"
    if not os.path.exists(ledger):
        notes.append(f"ledger not found: {ledger}")
    else:
        try:
            import yaml
            data = yaml.safe_load(open(ledger, encoding="utf-8"))
            for node in data.get("nodes", []):
                if node.get("status") == "red":
                    crate = (node.get("rust") or {}).get("crate")
                    if crate:
                        return crate, notes
            notes.append("ledger has no red node with rust.crate")
        except Exception as e:
            notes.append(f"ledger fallback failed: {type(e).__name__}: {e}")
    return None, notes


def run(cmd: list[str], timeout: int = 840) -> dict:
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return {"result": "pass" if proc.returncode == 0 else "fail",
                "detail": (proc.stdout + proc.stderr)[-800:]}
    except Exception as e:  # 起動できない・タイムアウト等は fail として報告する
        return {"result": "fail", "detail": f"{type(e).__name__}: {e}"}


def crate_dir(crate: str) -> str | None:
    """cargo metadata から、パッケージ名に対応するディレクトリを引く。"""
    try:
        proc = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"],
                              capture_output=True, text=True, timeout=120)
        for package in json.loads(proc.stdout).get("packages", []):
            if package.get("name") == crate:
                # チェッカーのパス述語は相対パスで評価する。親ディレクトリの名前に左右されないようにする。
                return os.path.relpath(os.path.dirname(package["manifest_path"]))
    except Exception:
        return None
    return None


def run_syn_checker(crate: str) -> dict:
    if not SYN_CHECK_MANIFEST.exists() or shutil.which("cargo") is None:
        return {"result": "not_installed", "detail": ""}
    target = crate_dir(crate)
    if target is None:
        return {"result": "fail", "detail": f"cannot find the directory of crate {crate}"}
    cmd = ["cargo", "run", "--quiet", "--release", "--locked",
           "--manifest-path", str(SYN_CHECK_MANIFEST), "--", "--format", "json", target]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=840)
    except Exception as e:
        return {"result": "fail", "detail": f"{type(e).__name__}: {e}"}
    if proc.returncode == 0:
        return {"result": "pass", "detail": ""}
    try:
        report = json.loads(proc.stdout)
    except ValueError:  # チェッカー自体のビルド失敗や使い方の誤り。JSON は出ない
        return {"result": "fail", "detail": (proc.stdout + proc.stderr)[-800:]}
    lines = [f"{v['path']}:{v['line']}:{v['column']}: {v['rule']}: {v['message']}"
             for v in report.get("violations", [])]
    lines += [f"{f['path']}: parse failure: {f['message']}"
              for f in report.get("parse_failures", [])]
    return {"result": "fail", "detail": "\n".join(lines)[:800]}


def main() -> int:
    crate, notes = parse_crate()
    if not crate:
        print(json.dumps({"all_pass": False,
                          "detail": "cannot determine target crate: " + "; ".join(notes)}))
        return 0

    gates = {
        "test": run(["cargo", "test", "-p", crate]),
        "clippy": run(["cargo", "clippy", "-p", crate, "--", "-D", "warnings"]),
        "fmt": run(["cargo", "fmt", "--check"]),
        "syn_checker": run_syn_checker(crate),
        "aislop": {"result": "not_installed", "detail": ""},
    }
    all_pass = all(g["result"] in ("pass", "not_installed") for g in gates.values())
    print(json.dumps({"all_pass": all_pass, "crate": crate, "gates": gates},
                     ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
