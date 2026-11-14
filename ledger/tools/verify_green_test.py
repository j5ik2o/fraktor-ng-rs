#!/usr/bin/env python3
"""verify_green.py の syn_checker ゲートを、スタブのワークスペースで確かめる。

使い方:
    python3 ledger/tools/verify_green_test.py

一時ディレクトリに 4 クレートのワークスペースを作り、verify_green.py を 5 通りに走らせる。
  1. 違反の無いクレート        syn_checker が pass で、all_pass が true
  2. 違反のあるクレート        syn_checker だけが fail で、all_pass が false
  3. チェッカーが無い配置      syn_checker が not_installed で、all_pass が true
  4. tests/ に関数内の use     syn_checker が pass（use-placement はテストファイルを見ない）
  5. src/ に関数内の use       syn_checker だけが fail で、use-placement が報告される
cargo と、tools/syn-check のビルドに要る依存の取得ができる環境で走らせること。
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

VERIFY_GREEN = Path(__file__).resolve().parent / "verify_green.py"

# aislop ゲートは `node scripts/check-aislop.mjs` の終了コードで合否を決める。
# このテストは syn_checker の挙動を見るものなので、aislop は常に通す偽の
# check-aislop.mjs を各ワークスペースに置く（aislop 自体は
# verify_green_test_aislop.py が確かめる）。
PASSING_CHECK_AISLOP = "process.exit(0);\n"

CLEAN_LIB = "pub struct Clean;\n"
# 公開型が 2 つあるので type-per-file に違反する。cargo test、clippy、fmt は通る。
DIRTY_LIB = "pub struct First;\n\npub struct Second;\n"
# 関数の中に use がある。同じ書き方を、テストファイルと lib 側のファイルに置く。
LOCAL_USE_TEST = (
    "#[test]\n"
    "fn ported_case() {\n"
    "    use std::collections::HashMap;\n"
    "\n"
    "    let map: HashMap<u8, u8> = HashMap::new();\n"
    "    assert!(map.is_empty());\n"
    "}\n"
)
LOCAL_USE_LIB = (
    "pub fn count() -> usize {\n"
    "    use std::collections::HashMap;\n"
    "\n"
    "    let map: HashMap<u8, u8> = HashMap::new();\n"
    "    map.len()\n"
    "}\n"
)


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def make_workspace(root: Path) -> None:
    crates = (("stub-clean", CLEAN_LIB), ("stub-dirty", DIRTY_LIB),
              ("stub-test-use", CLEAN_LIB), ("stub-lib-use", LOCAL_USE_LIB))
    members = ", ".join(f'"crates/{name}"' for name, _ in crates)
    write(root / "Cargo.toml", f'[workspace]\nmembers = [{members}]\nresolver = "3"\n')
    for name, lib in crates:
        write(root / "crates" / name / "Cargo.toml",
              f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n')
        write(root / "crates" / name / "src" / "lib.rs", lib)
    write(root / "crates" / "stub-test-use" / "tests" / "port.rs", LOCAL_USE_TEST)
    write(root / "scripts" / "check-aislop.mjs", PASSING_CHECK_AISLOP)


def run_verify(script: Path, workspace: Path, crate: str) -> dict:
    response = workspace / "previous-response.txt"
    response.write_text(f"crate: {crate}\n", encoding="utf-8")
    env = dict(os.environ, TAKT_PREVIOUS_RESPONSE_FILE=str(response))
    proc = subprocess.run([sys.executable, str(script)], cwd=workspace, env=env,
                          capture_output=True, text=True, check=True)
    return json.loads(proc.stdout)


def gate_results(result: dict) -> dict:
    return {name: gate["result"] for name, gate in result["gates"].items()}


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary) / "workspace"
        make_workspace(workspace)

        clean = run_verify(VERIFY_GREEN, workspace, "stub-clean")
        assert clean["all_pass"] is True, clean
        assert gate_results(clean)["syn_checker"] == "pass", clean
        print("1. clean crate:", gate_results(clean), "all_pass =", clean["all_pass"])

        dirty = run_verify(VERIFY_GREEN, workspace, "stub-dirty")
        results = gate_results(dirty)
        assert dirty["all_pass"] is False, dirty
        assert results["syn_checker"] == "fail", dirty
        assert all(results[gate] == "pass" for gate in ("test", "clippy", "fmt")), dirty
        detail = dirty["gates"]["syn_checker"]["detail"]
        assert "type-per-file" in detail and "crates/stub-dirty/src/lib.rs:3:1" in detail, detail
        print("2. dirty crate:", results, "all_pass =", dirty["all_pass"])
        print("   detail:", detail.splitlines()[0][:110])

        # チェッカーの無い配置：スクリプトだけを別の場所に置くと、tools/syn-check が見つからない。
        lonely = Path(temporary) / "elsewhere" / "ledger" / "tools" / "verify_green.py"
        lonely.parent.mkdir(parents=True)
        shutil.copy(VERIFY_GREEN, lonely)
        write(lonely.parents[2] / "scripts" / "check-aislop.mjs", PASSING_CHECK_AISLOP)
        absent = run_verify(lonely, workspace, "stub-dirty")
        assert gate_results(absent)["syn_checker"] == "not_installed", absent
        assert absent["all_pass"] is True, absent
        print("3. no checker:", gate_results(absent), "all_pass =", absent["all_pass"])

        test_use = run_verify(VERIFY_GREEN, workspace, "stub-test-use")
        assert test_use["all_pass"] is True, test_use
        assert gate_results(test_use)["syn_checker"] == "pass", test_use
        print("4. use inside a test fn:", gate_results(test_use), "all_pass =", test_use["all_pass"])

        lib_use = run_verify(VERIFY_GREEN, workspace, "stub-lib-use")
        results = gate_results(lib_use)
        assert lib_use["all_pass"] is False, lib_use
        assert results["syn_checker"] == "fail", lib_use
        assert all(results[gate] == "pass" for gate in ("test", "clippy", "fmt")), lib_use
        detail = lib_use["gates"]["syn_checker"]["detail"]
        assert "use-placement" in detail and "crates/stub-lib-use/src/lib.rs:2:5" in detail, detail
        print("5. use inside a lib fn:", results, "all_pass =", lib_use["all_pass"])
        print("   detail:", detail.splitlines()[0][:110])
    print("ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
