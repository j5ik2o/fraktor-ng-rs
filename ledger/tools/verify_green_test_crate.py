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
  9. `--response` のパスが存在しても読み取りで例外になる場合は、`all_pass=false`
     で失敗し、`detail` にパスと例外種別が入る
  10. 前のノードの応答が `.takt/port-test-response.txt` に残っていても、今回の
      port-test の転記（`save_port_test_response.py`）が上書きし、verify_green は
      今回のノードの crate を検証する
  11. 今回の port-test の応答が欠けて転記に失敗したときは、古い応答も消え、
      verify_green は古い crate を検証せず応答ファイル欠落で失敗する
  12. 保存先がまだ無い初回でも転記は成功する

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
SAVE_RESPONSE = Path(__file__).resolve().parent / "save_port_test_response.py"
SAVED_RESPONSE = Path(".takt") / "port-test-response.txt"


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


def run_case(nodes: list[tuple[str, str, str | None]], *, response: str | None = None,
             stub: str | None = None) -> dict:
    """一時ワークスペースで `verify_green.py` を CLI 実行し、JSON の結果を返す。

    nodes は台帳の (id, status, crate)。response を渡すと `--response` の応答ファイルを、
    stub を渡すと cargo/node スタブを置く。
    """
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, nodes)
        args: list[str] = []
        path = make_stub_bin(workspace, stub) if stub else None
        if response is not None:
            response_path = workspace / "port-test-response.txt"
            response_path.write_text(response, encoding="utf-8")
            args = ["--response", str(response_path)]
        proc = run_main(workspace, *args, path=path)
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        return json.loads(proc.stdout)


def assert_failed(result: dict, *needles: str) -> None:
    """失敗 JSON の契約（all_pass=false・crate と gates 無し・detail の語）を確認する。"""
    assert result["all_pass"] is False, result
    assert "crate" not in result, result
    assert "gates" not in result, result
    for needle in needles:
        assert needle in result["detail"], result


def test_main_failure_json_for_ambiguous_red() -> None:
    # red 2 件・crate 未指定の CLI 実行は、all_pass=false で gates を持たず、
    # 候補の一覧を detail に含み、終了コード 0 で終わる（workflow の遷移が
    # all_pass を読むため、非 0 終了にしてはならない）。
    result = run_case([("other-node", "red", "crate-a"), ("this-node", "red", "crate-b")])
    assert_failed(result, "other-node", "crate-a", "this-node", "crate-b")
    print("7. main() ambiguous failure JSON:", result["detail"])


def test_main_uses_response_crate() -> None:
    # --response の crate が main() の success JSON の crate になり、ゲートが
    # そのクレートに対して走る。台帳の red が今回ノード 1 件で応答の node: と
    # 一致すれば、応答の crate を採用する（crate_source=response）。
    result = run_case([("other-node", "green", "crate-a"), ("this-node", "red", "crate-b")],
                      response="node: this-node\ncrate: crate-b\ntest_file: crates/crate-b/tests/x.rs\n",
                      stub="crate-b")
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
        assert_failed(result, str(missing))
    print("9. --response missing file fail:", result["detail"])


def test_main_fails_when_response_lacks_crate_line() -> None:
    # --response のファイルに crate: 行が無い場合も Fail Fast。台帳へは落ちない。
    result = run_case([("this-node", "red", "crate-b")],
                      response="node: this-node\ntest_file: crates/crate-b/tests/x.rs\n")
    assert_failed(result, "lacks a crate line")
    print("10. --response without crate line fail:", result["detail"])


def test_main_ledger_fallback_when_no_response_flag() -> None:
    # --response を渡さない従来経路は維持される。台帳の red 1 件の crate を採用し、
    # その解決元が crate_source に載る。
    result = run_case([("this-node", "red", "crate-b")], stub="crate-b")
    assert result["crate"] == "crate-b", result
    assert result["crate_source"] == "ledger", result
    print("11. ledger fallback without --response:", result["crate"], result["crate_source"])


def test_main_fails_when_response_cannot_be_read() -> None:
    # --response のパスが存在しても読み取りで例外（ここではディレクトリ）になる場合、
    # open()/read() を握りつぶさず、all_pass=false・gates 無し・パス入り detail で終わる。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("this-node", "red", "crate-b")])
        directory = workspace / "a-directory"
        directory.mkdir()
        proc = run_main(workspace, "--response", str(directory))
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert_failed(result, str(directory), "cannot read response file")
    print("12. --response read failure:", result["detail"])


def run_save(workspace: Path, previous_response: str) -> dict:
    """port-test の応答を TAKT_PREVIOUS_RESPONSE_FILE に置き、転記の工程を CLI 実行する。"""
    previous = workspace / "previous-response.txt"
    previous.write_text(previous_response, encoding="utf-8")
    env = {**os.environ, "TAKT_PREVIOUS_RESPONSE_FILE": str(previous)}
    proc = subprocess.run([sys.executable, str(SAVE_RESPONSE)],
                          cwd=workspace, env=env, capture_output=True, text=True)
    assert proc.returncode == 0, (proc.returncode, proc.stderr)
    return json.loads(proc.stdout)


def write_stale_response(workspace: Path) -> Path:
    """前のノード（other-node／crate-a）の転記結果が作業ツリーに残った状態を作る。"""
    stale = workspace / SAVED_RESPONSE
    stale.parent.mkdir(parents=True, exist_ok=True)
    stale.write_text("node: other-node\ncrate: crate-a\ntest_file: crates/crate-a/tests/x.rs\n",
                     encoding="utf-8")
    return stale


def test_stale_response_is_replaced_by_current_port_test() -> None:
    # 前のノードの応答が残っていても、今回の port-test の転記が上書きし、
    # verify_green は今回のノードの crate（crate-b）を検証する。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "green", "crate-a"), ("this-node", "red", "crate-b")])
        write_stale_response(workspace)
        saved = run_save(workspace, "node: this-node\ncrate: crate-b\ntest_file: crates/crate-b/tests/x.rs\n")
        assert saved["saved"] is True and saved["crate"] == "crate-b", saved
        proc = run_main(workspace, "--response", str(SAVED_RESPONSE), path=make_stub_bin(workspace, "crate-b"))
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        result = json.loads(proc.stdout)
        assert result["crate"] == "crate-b", result
        assert result["crate_source"] == "response", result
    print("13. stale response replaced by current port-test:", result["crate"])


def test_stale_response_is_discarded_when_current_port_test_is_incomplete() -> None:
    # 今回の port-test の応答に crate: 行が無いと転記は失敗し、古い応答も消す。
    # verify_green は残っていた crate-a を検証せず、応答ファイル欠落で失敗する。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        write_ledger(workspace, [("other-node", "red", "crate-a")])
        stale = write_stale_response(workspace)
        saved = run_save(workspace, "node: this-node\ntest_file: crates/crate-b/tests/x.rs\n")
        assert saved["saved"] is False and "crate" in saved["detail"], saved
        assert not stale.exists(), "stale response must be discarded"
        proc = run_main(workspace, "--response", str(SAVED_RESPONSE), path=make_stub_bin(workspace, "crate-a"))
        assert proc.returncode == 0, (proc.returncode, proc.stderr)
        assert_failed(json.loads(proc.stdout), "response file not found", str(SAVED_RESPONSE))
    print("14. stale response discarded on incomplete port-test:", saved["detail"])


def test_save_without_previous_target() -> None:
    # 保存先がまだ無い初回でも、転記は成功する。
    with tempfile.TemporaryDirectory() as temporary:
        workspace = Path(temporary)
        saved = run_save(workspace, "node: this-node\ncrate: crate-b\ntest_file: crates/crate-b/tests/x.rs\n")
        assert saved["saved"] is True, saved
        assert (workspace / SAVED_RESPONSE).read_text(encoding="utf-8").startswith("node: this-node\n")
    print("15. save without previous target:", saved["path"])


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
    test_main_fails_when_response_cannot_be_read()
    test_stale_response_is_replaced_by_current_port_test()
    test_stale_response_is_discarded_when_current_port_test_is_incomplete()
    test_save_without_previous_target()
    print("ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
