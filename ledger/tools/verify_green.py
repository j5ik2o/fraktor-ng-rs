#!/usr/bin/env python3
"""verify-green の機械ゲート（TAKT の system step から固定コマンドで呼ばれる）。

対象クレートは次の順で解決する（`resolve_crate`）:
  1. `--crate <name>` 引数
  2. `--response <path>` が指す応答ファイルの `crate:` 行
  3. 環境変数 TAKT_PREVIOUS_RESPONSE_FILE が指す応答ファイルの `crate:` 行
  4. 台帳 `ledger/nodes.yaml` のノード。crate は、`rust.crate` を持つ
     `status: red` ノードが 1 件のときだけ採用する。2 件以上は曖昧なので
     選ばずに失敗し、理由に候補の一覧を載せる。

`--response` が明示されているときは 2 で確定するか、そこでの失敗で打ち切る
（台帳へはフォールバックしない）。応答ファイルが読めない・`crate:` 行が無い
場合は、指定されたパスを理由に含めて失敗させる。後続の 3・4 を通るのは
`--crate` と `--response` のどちらも指定されていない場合だけである。
`--crate` の値欠落も 2 以降へ進まずに失敗する。

選ばれた crate の解決元は、JSON の `crate_source` に
`crate`／`response`／previous_response／ledger のいずれかで載る。応答由来か
台帳由来かを出力から判別できるようにするためである。

機械ゲートをすべて実行して結果を JSON で出す。
終了コードは常に 0 で、合否は JSON の all_pass が持つ。

ゲート:
  test     cargo test -p <crate>（クレート全体。既存テストの退行検出を含む）
  clippy   cargo clippy -p <crate> --all-targets -- -D warnings
  fmt      cargo fmt --check
  syn_checker  tools/syn-check を対象クレートのディレクトリに対して実行する。
               違反かパース不能のファイルがあれば fail。チェッカーが無い環境
               （tools/syn-check が無い、cargo が無い）では "not_installed" として
               報告し、合否には含めない。
  aislop  node scripts/aislop-check-bootstrap.mjs check をリポジトリルートで実行し、
          その終了コードをそのまま pass/fail に反映する。node や node_modules が無く検査を
          実行できない場合も fail として報告する（not_installed で合格にはしない）。
"""
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

SYN_CHECK_MANIFEST = Path(__file__).resolve().parents[2] / "tools" / "syn-check" / "Cargo.toml"
REPO_ROOT = Path(__file__).resolve().parents[2]


def crate_from_response(path: str, notes: list[str]) -> str | None:
    """応答ファイルの `crate:` 行を返す。読めない・無い場合は notes に理由を積む。"""
    if not path:
        notes.append("no response file to read a crate line")
        return None
    if not os.path.exists(path):
        notes.append(f"response file not found: {path}")
        return None
    text = open(path, encoding="utf-8").read()
    m = re.search(r"^crate:\s*(\S+)\s*$", text, re.MULTILINE)
    if not m:
        notes.append(f"response lacks a crate line: {path}")
        return None
    return m.group(1)


def crate_from_ledger(notes: list[str]) -> str | None:
    """台帳の red ノードの rust.crate を返す。

    `rust.crate` を持つ `status: red` ノードが 1 件のときだけ採用する。2 件以上は
    別ゴールや中断した実行の red と区別できず、選ぶと別クレートを検証してしまうため、
    候補の一覧を理由に積んで失敗させる。
    """
    ledger = "ledger/nodes.yaml"
    if not os.path.exists(ledger):
        notes.append(f"ledger not found: {ledger}")
        return None
    try:
        import yaml
        data = yaml.safe_load(open(ledger, encoding="utf-8"))
    except Exception as e:
        notes.append(f"ledger fallback failed: {type(e).__name__}: {e}")
        return None
    candidates = []
    for node in data.get("nodes", []):
        if node.get("status") == "red":
            crate = (node.get("rust") or {}).get("crate")
            if crate:
                candidates.append((node.get("id"), crate))
    if not candidates:
        notes.append("ledger has no red node with rust.crate")
        return None
    if len(candidates) > 1:
        listing = ", ".join(f"{node_id}={crate}" for node_id, crate in candidates)
        notes.append(f"ambiguous: {len(candidates)} red nodes with rust.crate ({listing}); pass --crate")
        return None
    return candidates[0][1]


def parse_args(argv: list[str]) -> tuple[str | None, str | None, list[str]]:
    """`--crate` と `--response` を取り出す。値の無い `--crate` は不正として理由を積む。"""
    crate: str | None = None
    response: str | None = None
    notes: list[str] = []
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--crate":
            if i + 1 >= len(argv) or argv[i + 1].startswith("--"):
                notes.append("--crate requires a crate name")
            else:
                crate = argv[i + 1]
                i += 1
        elif arg == "--response":
            if i + 1 >= len(argv) or argv[i + 1].startswith("--"):
                notes.append("--response requires a file path")
            else:
                response = argv[i + 1]
                i += 1
        i += 1
    return crate, response, notes


CRATE_SOURCES = ("crate", "response", "previous_response", "ledger")


def resolve_crate(argv: list[str]) -> tuple[str | None, str, str | None]:
    """対象クレート名、特定できなかった場合の理由、解決元を返す。

    `--crate` は明示指定として最優先。値の無い `--crate` と、値のある
    `--response` で crate を確定できない場合は、そこで Fail Fast にして
    台帳フォールバックへ無言で戻らない。`--response` 未指定のときだけ
    TAKT_PREVIOUS_RESPONSE_FILE と台帳へ進む。
    """
    explicit, response, notes = parse_args(argv)
    if explicit:
        return explicit, "; ".join(notes), "crate"
    if "--crate requires a crate name" in notes:
        return None, "; ".join(notes), None
    if response:
        crate = crate_from_response(response, notes)
        return crate, "; ".join(notes), ("response" if crate else None)
    crate = crate_from_response(os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", ""), notes)
    if crate:
        return crate, "; ".join(notes), "previous_response"
    crate = crate_from_ledger(notes)
    return crate, "; ".join(notes), ("ledger" if crate else None)



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


def run_aislop() -> dict:
    """aislop-check-bootstrap.mjs check を実行し、終了コードを pass/fail に反映する。

    スクリプトはカレントディレクトリ（呼び出し側のリポジトリルート）から解決する。
    他のゲート（cargo の実行と台帳の読み取り）もカレントディレクトリをリポジトリ
    ルートとして動く前提であり、テストは一時ワークスペースをカレントにして
    スタブの aislop-check-bootstrap.mjs を置ける。
    """
    if shutil.which("node") is None:
        return {"result": "fail", "detail": "node is not installed; cannot run scripts/aislop-check-bootstrap.mjs"}
    root = Path.cwd()
    script = root / "scripts" / "aislop-check-bootstrap.mjs"
    try:
        # 検査の内側の制限時間は、正本（aislop-check-v2）の scan が 600 秒、
        # bootstrap 全体が 660 秒である。外側が先に切れると内側の終了処理
        # （プロセスグループの終了を含む）が終わらないため、外側の 690 秒には
        # 内側の処理が終わるまでの余裕を持たせている。
        proc = subprocess.run(["node", str(script), "check"], capture_output=True, text=True,
                              cwd=str(root), timeout=690)
    except Exception as e:
        return {"result": "fail", "detail": f"{type(e).__name__}: {e}"}
    return {"result": "pass" if proc.returncode == 0 else "fail",
            "detail": (proc.stdout + proc.stderr)[-800:]}


def main(argv: list[str] | None = None) -> int:
    crate, notes, crate_source = resolve_crate(sys.argv[1:] if argv is None else argv)
    if not crate:
        print(json.dumps({"all_pass": False,
                          "detail": "cannot determine target crate: " + notes}))
        return 0

    gates = {
        "test": run(["cargo", "test", "-p", crate]),
        "clippy": run(["cargo", "clippy", "-p", crate, "--all-targets", "--", "-D", "warnings"]),
        "fmt": run(["cargo", "fmt", "--check"]),
        "syn_checker": run_syn_checker(crate),
        "aislop": run_aislop(),
    }
    all_pass = all(g["result"] in ("pass", "not_installed") for g in gates.values())
    print(json.dumps({"all_pass": all_pass, "crate": crate, "crate_source": crate_source,
                      "gates": gates},
                     ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
