#!/usr/bin/env python3
"""verify_green.py の対象クレート解決を、cargo を使わずに確かめる。

使い方:
    python3 ledger/tools/verify_green_test_crate.py

issue #65 の回帰を固定する。crate の指定が無いとき、台帳の先頭の red ノードを
無条件に返すと、別ゴールや中断した実行の red ノードが先頭にある場合に
別クレートを検証してしまう。ここでは次の契約を観測する:
  1. crate を直接指定したときは、その crate が選ばれる
     （台帳の先頭に別の red ノードがあっても左右されない）
  2. crate 指定が無く red ノードが 2 件以上のときは、曖昧に選ばず失敗する
     （候補の一覧を理由に含める）
  3. crate 指定が無く red ノードが 1 件のときは、その crate を採用する
  4. crate 指定が無く red ノードが 0 件のときは、失敗する
  5. crate 未指定・red 2 件以上で `main()` を実行すると、終了コード 0・
     `all_pass=false`・`gates` 無し・`detail` に候補一覧（外部契約の観測）
  6. `--response` を渡すと `main()` の JSON の `crate` が応答の crate になる
     （`crate_source` が `response`）
  7. `--response` のファイルが無い、または `crate:` 行が無い場合、台帳の red
     ノードへフォールバックせず `all_pass=false` で失敗し、`detail` にパスが入る
  8. `--response` を渡さない従来経路は、台帳の red 1 件の crate を採用する
     （`crate_source` が `ledger`）

crate の解決は cargo に依存しないので、cargo の無い環境でも走らせられる。
`main()` の成功経路は cargo と node をスタブに置いて観測する。
"""
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

VERIFY_GREEN = Path(__file__).resolve().parent / "verify_green.py"


def load_module():
    spec = importlib.util.spec_from_file_location("verify_green_under_test", VERIFY_GREEN)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_ledger(root: Path, nodes: list[tuple[str, str, str | None]]) -> None:
    """(id, status, crate) の並びを台帳として書き出す。crate が None なら rust.crate を null にする。"""
    lines = ["nodes:"]
    for node_id, status, crate in nodes:
        lines.append(f"- id: {node_id}")
        lines.append(f"  status: {status}")
        lines.append("  rust:")
        lines.append(f"    crate: {crate if crate is not None else 'null'}")
    (root / "ledger").mkdir(parents=True, exist_ok=True)
    (root / "ledger" / "nodes.yaml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_resolve(module, workspace: Path, *, argv: list[str] | None = None,
                previous_response: str | None = None) -> tuple[str | None, str]:
    """ワークスペースをカレントにして crate 解決を実行する。

    戻り値は (解決した crate, 理由の連結)。テストのために環境を汚さないよう、
    TAKT_PREVIOUS_RESPONSE_FILE は都度付け外しする。
    """
    cwd = os.getcwd()
    had_previous = "TAKT_PREVIOUS_RESPONSE_FILE" in os.environ
    saved_previous = os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE")
    os.chdir(workspace)
    try:
        if previous_response is None:
            os.environ.pop("TAKT_PREVIOUS_RESPONSE_FILE", None)
        else:
            response = workspace / "previous-response.txt"
            response.write_text(previous_response, encoding="utf-8")
            os.environ["TAKT_PREVIOUS_RESPONSE_FILE"] = str(response)
        crate, reason, _source = module.resolve_crate(argv or [])
        return crate, reason
    finally:
        os.chdir(cwd)
        if had_previous:
            os.environ["TAKT_PREVIOUS_RESPONSE_FILE"] = saved_previous
        else:
            os.environ.pop("TAKT_PREVIOUS_RESPONSE_FILE", None)


def test_explicit_crate_wins_over_ledger_head(module) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "red", "crate-a"),
                                 ("this-node", "red", "crate-b")])
        crate, reason = run_resolve(module, workspace, argv=["--crate", "crate-b"])
        assert crate == "crate-b", (crate, reason)
    print("1. explicit --crate:", crate)


def test_explicit_crate_without_ledger(module) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)  # 台帳が無くても指定 crate を使う
        crate, reason = run_resolve(module, workspace, argv=["--crate", "crate-b"])
        assert crate == "crate-b", (crate, reason)
    print("2. explicit --crate without ledger:", crate)


def test_two_red_nodes_without_crate_fail_with_candidates(module) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "red", "crate-a"),
                                 ("this-node", "red", "crate-b")])
        crate, reason = run_resolve(module, workspace)
        assert crate is None, (crate, reason)
        assert "crate-a" in reason and "crate-b" in reason, reason
    print("3. two red nodes without crate fail:", reason)


def test_single_red_node_without_crate_is_used(module) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("green-node", "green", "crate-green"),
                                 ("this-node", "red", "crate-b")])
        crate, reason = run_resolve(module, workspace)
        assert crate == "crate-b", (crate, reason)
    print("4. single red node without crate:", crate)


def test_no_red_node_without_crate_fails(module) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("green-node", "green", "crate-green")])
        crate, reason = run_resolve(module, workspace)
        assert crate is None, (crate, reason)
    print("5. no red node without crate fail:", reason)


def test_missing_explicit_crate_value_is_rejected(module) -> None:
    # 値の無い --crate を黙って読み飛ばすと、指定ミスが台帳の red ノードへの
    # 無言のフォールバックに化ける。対象クレートを指定する意志があるのに
    # 値が無いのは呼び出し側の誤りなので、Fail Fast で失敗させる。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("this-node", "red", "crate-b")])
        crate, reason = run_resolve(module, workspace, argv=["--crate"])
        assert crate is None, (crate, reason)
        assert "crate" in reason, reason
    print("6. --crate without a value fail:", reason)


def run_main(workspace: Path, *args: str, path: str | None = None) -> subprocess.CompletedProcess:
    """ワークスペースをカレントにして verify_green.py を CLI として実行する。

    main() の外部契約（stdout の JSON と終了コード）を観測する。Takt と同じく
    TAKT_PREVIOUS_RESPONSE_FILE は外した環境で起動する。
    """
    env = {k: v for k, v in os.environ.items() if k != "TAKT_PREVIOUS_RESPONSE_FILE"}
    if path is not None:
        env["PATH"] = path
    return subprocess.run([sys.executable, str(VERIFY_GREEN), *args],
                          cwd=workspace, env=env, capture_output=True, text=True)


def make_stub_bin(root: Path, crate: str) -> str:
    """ゲートが cargo と node を起動する成功経路を観測するためのスタブを置く。

    cargo は metadata で対象クレートのディレクトリを返し、他のサブコマンドは
    成功する。node も成功する。元の PATH の実行物はシンボリックリンクで残す。
    """
    bindir = root / "test-bin"
    bindir.mkdir(parents=True, exist_ok=True)
    (bindir / "cargo").write_text(
        "#!/bin/sh\n"
        'case "$1" in\n'
        f"  metadata) echo '{{\"packages\":[{{\"name\":\"{crate}\",\"manifest_path\":\"{root}/crates/{crate}/Cargo.toml\"}}]}}' ;;\n"
        "esac\n"
        "exit 0\n",
        encoding="utf-8",
    )
    (bindir / "node").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    for name in ("cargo", "node"):
        (bindir / name).chmod(0o755)
    for name in ("sh", "env", "dirname", "cat"):
        target = shutil.which(name)
        if target:
            (bindir / name).symlink_to(target)
    return str(bindir)


def test_main_failure_json_for_ambiguous_red() -> None:
    # red 2 件・crate 未指定の CLI 実行は、all_pass=false で gates を持たず、
    # 候補の一覧を detail に含み、終了コード 0 で終わる（workflow の遷移が
    # all_pass を読むため、非 0 終了にしてはならない）。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "red", "crate-a"),
                                 ("this-node", "red", "crate-b")])
        proc = run_main(workspace)
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["all_pass"] is False, result
        assert "gates" not in result, result
        assert "other-node" in result["detail"] and "crate-a" in result["detail"], result
        assert "this-node" in result["detail"] and "crate-b" in result["detail"], result
    print("7. main() ambiguous failure JSON:", result["detail"])


def test_main_uses_response_crate() -> None:
    # --response の crate が main() の success JSON の crate になり、ゲートが
    # そのクレートに対して走る。台帳の先頭 red が別クレートでも左右されない。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "red", "crate-a"),
                                 ("this-node", "red", "crate-b")])
        response = workspace / "port-test-response.txt"
        response.write_text("node: this-node\ncrate: crate-b\ntest_file: crates/crate-b/tests/x.rs\n",
                            encoding="utf-8")
        path = make_stub_bin(workspace, "crate-b")
        proc = run_main(workspace, "--response", str(response), path=path)
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["crate"] == "crate-b", result
        assert result["crate_source"] == "response", result
        assert set(result["gates"]) == {"test", "clippy", "fmt", "syn_checker", "aislop"}, result
    print("8. main() uses --response crate:", result["crate"], result["all_pass"])


def test_main_fails_when_response_file_missing() -> None:
    # --response のファイルが無い場合、台帳の red ノードの crate へ無言で
    # フォールバックせず失敗する。台帳に red（crate-b）があっても all_pass にしない。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("this-node", "red", "crate-b")])
        missing = workspace / "does-not-exist.txt"
        proc = run_main(workspace, "--response", str(missing))
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["all_pass"] is False, result
        assert "crate" not in result, result
        assert "gates" not in result, result
        assert str(missing) in result["detail"], result
    print("9. --response missing file fail:", result["detail"])


def test_main_fails_when_response_lacks_crate_line() -> None:
    # --response のファイルに crate: 行が無い場合も Fail Fast。台帳へは落ちない。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("this-node", "red", "crate-b")])
        response = workspace / "port-test-response.txt"
        response.write_text("node: this-node\ntest_file: crates/crate-b/tests/x.rs\n",
                            encoding="utf-8")
        proc = run_main(workspace, "--response", str(response))
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["all_pass"] is False, result
        assert "gates" not in result, result
        assert str(response) in result["detail"], result
        assert "lacks a crate line" in result["detail"], result
    print("10. --response without crate line fail:", result["detail"])


def test_main_ledger_fallback_when_no_response_flag() -> None:
    # --response を渡さない従来経路は維持される。台帳の red 1 件の crate を採用し、
    # その解決元が crate_source に載る。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("this-node", "red", "crate-b")])
        path = make_stub_bin(workspace, "crate-b")
        proc = run_main(workspace, path=path)
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["crate"] == "crate-b", result
        assert result["crate_source"] == "ledger", result
    print("11. ledger fallback without --response:", result["crate"], result["crate_source"])


def main() -> int:
    module = load_module()
    test_explicit_crate_wins_over_ledger_head(module)
    test_explicit_crate_without_ledger(module)
    test_two_red_nodes_without_crate_fail_with_candidates(module)
    test_single_red_node_without_crate_is_used(module)
    test_no_red_node_without_crate_fails(module)
    test_missing_explicit_crate_value_is_rejected(module)
    test_main_failure_json_for_ambiguous_red()
    test_main_uses_response_crate()
    test_main_fails_when_response_file_missing()
    test_main_fails_when_response_lacks_crate_line()
    test_main_ledger_fallback_when_no_response_flag()
    print("ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
