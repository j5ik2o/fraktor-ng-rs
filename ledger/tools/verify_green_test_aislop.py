#!/usr/bin/env python3
"""verify_green.py の aislop ゲートを、スタブのワークスペースで確かめる。

使い方:
    python3 ledger/tools/verify_green_test_aislop.py

aislop の結果は `node scripts/check-aislop.mjs` が決める。このテストでは
`node` と、`scripts/check-aislop.mjs` が実行する「偽の aislop 実行物」を
一時ワークスペースに置き、確かめられる範囲を固定する:
  1. check-aislop が成功するツリー      aislop が pass
  2. check-aislop が失敗するツリー      aislop が fail
  3. check-aislop.mjs が無いツリー      aislop は not_installed ではなく fail
                                        （組み込み未完の状態を合格にしない）
  4. node が無い環境                    aislop は not_installed ではなく fail
                                        （実行物が無い状態を合格にしない）
cargo を使わないよう、対象クレートは決め打ちせず、`test`/`clippy`/`fmt` を
走らせない配置（`cargo` を PATH から外す）で aislop ゲートだけを観測する。
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

VERIFY_GREEN = Path(__file__).resolve().parent / "verify_green.py"

# 偽の aislop: check-aislop.mjs は aislop を node_modules/.bin/aislop から起動する。
# ここでは check-aislop.mjs 自体も検証しない（その契約は check-aislop.test.mjs）。
# verify_green が見るのは、check-aislop.mjs の終了コードだけである。
FAKE_CHECK_AISLOP = """#!/usr/bin/env node
process.exit(Number(process.env.FAKE_CHECK_AISLOP_EXIT || "0"));
"""


def write(path: Path, text: str, mode: int | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    if mode is not None:
        path.chmod(mode)


def make_workspace(root: Path, with_script: bool) -> None:
    """verify_green.py を repo 直下の ledger/tools/ に置いた最小ツリー。

    with_script=False は、node_modules の準備が済んでいない作業ツリー
    （scripts/check-aislop.mjs が無い）を模す。この状態でも aislop は
    not_installed に落とさず fail として報告する。
    """
    write(root / "Cargo.toml", '[workspace]\nmembers = []\nresolver = "3"\n')
    (root / "ledger" / "tools").mkdir(parents=True, exist_ok=True)
    shutil.copy(VERIFY_GREEN, root / "ledger" / "tools" / "verify_green.py")
    if with_script:
        write(root / "scripts" / "check-aislop.mjs", FAKE_CHECK_AISLOP, 0o755)


def run_verify(root: Path, crate: str, check_exit: int, hide: tuple[str, ...] = ("cargo",)) -> dict:
    response = root / "previous-response.txt"
    response.write_text(f"crate: {crate}\n", encoding="utf-8")
    env = dict(os.environ, TAKT_PREVIOUS_RESPONSE_FILE=str(response),
               FAKE_CHECK_AISLOP_EXIT=str(check_exit))
    # 隠す実行物だけを外した PATH を作る。実行物のあるディレクトリごと外すと、
    # 同じディレクトリにある別の実行物（例えば /usr/bin の cargo と node）まで
    # 消えてしまうため、残す実行物へのシンボリックリンクだけを置いた専用の
    # ディレクトリを PATH にする。
    bindir = root / "test-bin"
    bindir.mkdir(parents=True, exist_ok=True)
    for name in ("cargo", "node"):
        if name in hide:
            continue
        target = shutil.which(name)
        if target:
            (bindir / name).symlink_to(target)
    env["PATH"] = str(bindir)
    proc = subprocess.run([sys.executable, str(root / "ledger" / "tools" / "verify_green.py")],
                          cwd=root, env=env, capture_output=True, text=True)
    assert proc.returncode == 0, proc.stderr
    return json.loads(proc.stdout)


def gate_results(result: dict) -> dict:
    return {name: gate["result"] for name, gate in result["gates"].items()}


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        temporary = Path(temporary)

        passing = temporary / "passing"
        make_workspace(passing, with_script=True)
        ok = run_verify(passing, "stub-clean", check_exit=0)
        results = gate_results(ok)
        assert results["aislop"] == "pass", ok
        print("1. check-aislop passes:", results)

        failing = temporary / "failing"
        make_workspace(failing, with_script=True)
        bad = run_verify(failing, "stub-clean", check_exit=1)
        results = gate_results(bad)
        assert results["aislop"] == "fail", bad
        print("2. check-aislop fails:", results)

        missing = temporary / "missing"
        make_workspace(missing, with_script=False)
        absent = run_verify(missing, "stub-clean", check_exit=0)
        results = gate_results(absent)
        assert results["aislop"] != "not_installed", absent
        assert results["aislop"] == "fail", absent
        print("3. unprepared tree (no scripts/check-aislop.mjs):", results)

        # node が無い環境: check-aislop.mjs を実行できないので fail。
        # not_installed で合格にしない。cargo と node の両方を PATH から外す。
        no_node = temporary / "no-node"
        make_workspace(no_node, with_script=True)
        without_node = run_verify(no_node, "stub-clean", check_exit=0, hide=("cargo", "node"))
        results = gate_results(without_node)
        assert results["aislop"] == "fail", without_node
        assert results["aislop"] != "not_installed", without_node
        assert "node" in without_node["gates"]["aislop"]["detail"], without_node["gates"]["aislop"]
        print("4. no node:", results)
    print("ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
