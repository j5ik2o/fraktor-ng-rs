#!/usr/bin/env python3
"""準備スクリプトの判定を、テスト用の小さな git repo で確かめる。

使い方:
    python3 tools/pekko-source/prepare_pekko_source_test.py

確かめること:
  1. pekko/ が無い            ローカルの原典から clone し、HEAD が期待 commit になる（exit 0）
  2. 正しく揃っている         何もせず exit 0。やり直しても 0、目印のファイルが残る（冪等）
  3. HEAD が違う（既存）      消さずに exit 0 以外
     3b. tag が別の commit を指す
                              用意した後の HEAD 照合で exit 0 以外
  4. 壊れている               空のディレクトリ／HEAD を解決できない repo を作り直す
  4b. ローカル原典が壊れている
                              上流へ切り替える
  5. ローカル原典が無い       既定の上流 URL から浅く clone する
                              （git の URL 書き換えでテスト用 repo へ向ける）
  6. 環境変数                 PEKKO_SOURCE_LOCAL_REPO の上書きに従う（既定のパスを使わない）
  7. 置き場所                 スクリプトのディレクトリではなく、cwd の pekko/ に置く

すべてのケースで、環境から HOME を除いて実行し、stdout には何も出さないことを確かめる。
stdout は TAKT が KEY=VALUE として読むため（dist/core/runtime/runtime-environment.js の
parseScriptOutput）、値を出すと run が止まるか無関係な環境変数を注入してしまう。
"""
import os
import shutil
import subprocess
import tempfile
import traceback
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "prepare_pekko_source.sh"

# テスト用の小さな repo では tag v1.7.1 がこの commit を指す。期待 commit は
# PEKKO_SOURCE_EXPECTED_COMMIT で差し替える（既定は Pekko v1.7.1 の 9a54d129…）。
GIT_IDENTITY = ["-c", "user.name=takt-test", "-c", "user.email=takt-test@example.invalid"]

# 存在しないローカル原典・上流。ネットワークへ出ないための既定値。
MISSING_LOCAL = "/nonexistent/pekko-source-for-test"
MISSING_UPSTREAM = "/nonexistent/pekko-upstream-for-test"

# スクリプトが持つ上流の既定値（order.md 仕様1-2-2）。
DEFAULT_UPSTREAM = "https://github.com/apache/pekko"


def git(*args: str, cwd: str | Path, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *GIT_IDENTITY, *args],
        cwd=str(cwd),
        capture_output=True,
        text=True,
        timeout=60,
    )
    if check and result.returncode != 0:
        raise AssertionError(f"git {' '.join(args)} failed: {result.stderr}")
    return result.stdout.strip()


def make_repo(path: str | Path) -> str:
    """1 つだけ commit を持つ repo を作り、その commit を返す。"""
    os.makedirs(path, exist_ok=True)
    git("init", "-q", cwd=path)
    marker = Path(path) / "marker.txt"
    marker.write_text("v1.7.1\n", encoding="utf-8")
    git("add", "marker.txt", cwd=path)
    git("commit", "-q", "-m", "v1.7.1", cwd=path)
    return git("rev-parse", "HEAD", cwd=path)


def make_source(path: str | Path) -> str:
    """tag v1.7.1 が指す commit を返す。"""
    head = make_repo(path)
    git("tag", "v1.7.1", cwd=path)
    return head


def make_source_with_older_tag(path: str | Path) -> str:
    """tag v1.7.1 が最新でない commit を指す原典を作る。"""
    make_repo(path)
    git("tag", "v1.7.1", cwd=path)
    marker = Path(path) / "marker.txt"
    marker.write_text("newer\n", encoding="utf-8")
    git("commit", "-q", "-am", "newer", cwd=path)
    return git("rev-parse", "HEAD", cwd=path)


def make_worktree(root: str | Path) -> str:
    """作業ツリー。pekko/ は含まない。"""
    path = Path(root) / "worktree"
    os.makedirs(path, exist_ok=True)
    git("init", "-q", cwd=path)
    (path / "README.md").write_text("worktree\n", encoding="utf-8")
    return str(path)


def base_env(**overrides: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if k != "HOME"}
    env.update(
        {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "PEKKO_SOURCE_LOCAL_REPO": MISSING_LOCAL,
            "PEKKO_SOURCE_UPSTREAM_URL": MISSING_UPSTREAM,
        }
    )
    env.update(overrides)
    return env


def upstream_env(upstream: str | Path, **overrides: str) -> dict[str, str]:
    """既定の上流 URL を使わせ、git の URL 書き換えでテスト用 repo へ向ける。

    PEKKO_SOURCE_UPSTREAM_URL を設定しないので、スクリプトは既定値
    https://github.com/apache/pekko を使う。その値を url.<file URL>.insteadOf で
    テスト用 repo の file:// URL に書き換える。GIT_ALLOW_PROTOCOL=file でネットワークを
    遮断するので、書き換えを通らない URL（例: file://https://…）は失敗する。
    """
    env = base_env(**overrides)
    env.pop("PEKKO_SOURCE_UPSTREAM_URL", None)
    env.update(
        {
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": f"url.{Path(upstream).as_uri()}.insteadOf",
            "GIT_CONFIG_VALUE_0": DEFAULT_UPSTREAM,
            "GIT_ALLOW_PROTOCOL": "file",
        }
    )
    return env


def run_script(cwd: str | Path, env: dict[str, str]) -> subprocess.CompletedProcess:
    """作業ツリーを cwd にして、TAKT と同じく bash でスクリプトを実行する。"""
    if not SCRIPT.exists():
        raise FileNotFoundError(f"prepare script is not implemented yet: {SCRIPT}")
    return subprocess.run(
        ["bash", str(SCRIPT)],
        cwd=str(cwd),
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )


def assert_silent_stdout(result: subprocess.CompletedProcess, label: str) -> None:
    assert result.stdout == "", f"{label}: stdout must be empty, got {result.stdout!r}"


def assert_pekko_at(path: str | Path, expected_commit: str, label: str) -> None:
    target = Path(path) / "pekko"
    assert target.is_dir(), f"{label}: pekko/ must exist"
    assert (target / ".git").exists(), f"{label}: pekko/.git must exist"
    actual = git("rev-parse", "HEAD", cwd=target)
    assert actual == expected_commit, f"{label}: HEAD must be {expected_commit}, got {actual}"


def test_absent_clones_from_local_source() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        result = run_script(
            worktree,
            base_env(
                PEKKO_SOURCE_LOCAL_REPO=str(source),
                PEKKO_SOURCE_EXPECTED_COMMIT=expected,
            ),
        )
        assert result.returncode == 0, f"absent: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "absent")
        assert_pekko_at(worktree, expected, "absent")
        # tag を指定せず clone すると、HEAD が v1.7.1 でなくても通ってしまう。
        assert git("rev-parse", "v1.7.1", cwd=Path(worktree) / "pekko") == expected, (
            "absent: cloned pekko must carry the v1.7.1 tag"
        )


def test_present_and_correct_is_noop() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        env = base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected)

        first = run_script(worktree, env)
        assert first.returncode == 0, f"present: first run expected 0, got {first.returncode}; stderr={first.stderr!r}"
        assert_silent_stdout(first, "present first")

        marker = Path(worktree) / "pekko" / "keep-me.txt"
        marker.write_text("kept\n", encoding="utf-8")
        # 2 回目は原典を消しても、clone し直さなければ成功する（毎回作り直す実装を拒否する）。
        shutil.rmtree(source)

        second = run_script(worktree, env)
        assert second.returncode == 0, f"present: second run expected 0, got {second.returncode}; stderr={second.stderr!r}"
        assert_silent_stdout(second, "present second")
        assert marker.exists(), "present: an already correct pekko/ must not be re-cloned"
        assert_pekko_at(worktree, expected, "present")


def test_existing_pekko_with_other_head_is_not_removed() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        git("commit", "-q", "--allow-empty", "-m", "outer", cwd=worktree)
        wrong = git("rev-parse", "HEAD", cwd=worktree)

        git("clone", "-q", "--branch", "v1.7.1", str(source), str(Path(worktree) / "pekko"), cwd=root)
        git("checkout", "-q", expected, cwd=Path(worktree) / "pekko")
        (Path(worktree) / "pekko" / "keep-me.txt").write_text("kept\n", encoding="utf-8")

        result = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=wrong),
        )
        assert result.returncode != 0, "mismatch: expected a non-zero exit, got 0"
        assert_silent_stdout(result, "mismatch")
        assert result.stderr.strip(), "mismatch: stderr must explain the actual and expected HEAD"
        assert (Path(worktree) / "pekko" / "keep-me.txt").exists(), (
            "mismatch: an existing correct repo must not be removed"
        )
        assert git("rev-parse", "HEAD", cwd=Path(worktree) / "pekko") == expected, (
            "mismatch: an existing correct repo must keep its HEAD"
        )


def test_source_tag_pointing_elsewhere_fails_after_prepare() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source_with_older_tag(source)
        worktree = make_worktree(root)
        result = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode != 0, "wrong tag: expected a non-zero exit, got 0"
        assert_silent_stdout(result, "wrong tag")


def test_broken_empty_directory_is_rebuilt() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        # 作業ツリー自体が git repo なので、空の pekko/ で rev-parse HEAD を実行すると
        # 外側の HEAD が返る。その値を「正しい」と誤認しないことを確かめる。
        git("commit", "-q", "--allow-empty", "-m", "outer", cwd=worktree)
        os.makedirs(Path(worktree) / "pekko")

        result = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode == 0, f"empty dir: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "empty dir")
        assert_pekko_at(worktree, expected, "empty dir")


def test_broken_repo_without_head_is_rebuilt() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        broken = Path(worktree) / "pekko"
        os.makedirs(broken)
        git("init", "-q", cwd=broken)
        (broken / "stale.txt").write_text("stale\n", encoding="utf-8")

        result = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode == 0, f"no HEAD: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "no HEAD")
        assert_pekko_at(worktree, expected, "no HEAD")
        assert not (broken / "stale.txt").exists(), "no HEAD: the broken pekko/ must be replaced, not reused"


def test_broken_local_source_falls_back_to_upstream() -> None:
    with tempfile.TemporaryDirectory() as root:
        upstream = Path(root) / "upstream"
        expected = make_source(upstream)
        broken_local = Path(root) / "broken-local"
        os.makedirs(broken_local)
        (broken_local / "not-a-repo.txt").write_text("x\n", encoding="utf-8")
        worktree = make_worktree(root)

        result = run_script(
            worktree,
            upstream_env(upstream, PEKKO_SOURCE_LOCAL_REPO=str(broken_local), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode == 0, f"fallback: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "fallback")
        assert_pekko_at(worktree, expected, "fallback")


def test_missing_local_source_uses_shallow_upstream_clone() -> None:
    with tempfile.TemporaryDirectory() as root:
        upstream = Path(root) / "upstream"
        expected = make_source(upstream)
        # tag v1.7.1 を 2 つ目の commit に移す。--branch だけの clone でも履歴は 2 件に
        # なるので、履歴を 1 件に切るかどうかで --depth 1 の有無を観測できる。
        (upstream / "marker.txt").write_text("next\n", encoding="utf-8")
        git("commit", "-q", "-am", "next", cwd=upstream)
        git("tag", "-f", "v1.7.1", cwd=upstream)
        expected = git("rev-parse", "v1.7.1", cwd=upstream)
        worktree = make_worktree(root)

        result = run_script(
            worktree,
            upstream_env(upstream, PEKKO_SOURCE_LOCAL_REPO=MISSING_LOCAL, PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode == 0, f"upstream: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "upstream")
        assert_pekko_at(worktree, expected, "upstream")
        depth = git("rev-list", "--count", "HEAD", cwd=Path(worktree) / "pekko")
        assert depth == "1", f"upstream: the upstream clone must be shallow (--depth 1), got {depth} commit(s)"


def test_local_source_override_is_honoured() -> None:
    with tempfile.TemporaryDirectory() as root:
        used = Path(root) / "used-source"
        make_source(used)
        unused = Path(root) / "unused-source"
        make_source(unused)
        # 原典ごとに異なる内容を含め、上書き先を commit で区別できるようにする。
        (used / "marker.txt").write_text("used-source\n", encoding="utf-8")
        git("commit", "-q", "-am", "used only", cwd=used)
        git("tag", "-f", "v1.7.1", cwd=used)
        expected = git("rev-parse", "v1.7.1", cwd=used)
        worktree = make_worktree(root)

        result = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(used), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result.returncode == 0, f"override: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "override")
        assert_pekko_at(worktree, expected, "override")


def test_pekko_is_not_placed_in_the_callers_directory() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        # スクリプトを、呼び出し元のディレクトリに隔離して置く。$0 のディレクトリ基準で
        # 相対パスを解決する実装は、このディレクトリに pekko/ を作る。
        stacked = Path(root) / "stacked"
        os.makedirs(stacked)
        shutil.copy(SCRIPT, stacked / SCRIPT.name)
        script = stacked / SCRIPT.name

        # bash を呼ぶプロセスの cwd を作業ツリーにし、スクリプトを相対パスで渡す。
        # こうすると、$0 のディレクトリ基準と cwd 基準が一致せず、どちらを使うかが観測できる。
        relative = os.path.relpath(script, start=worktree)
        result = subprocess.run(
            ["bash", relative],
            cwd=worktree,
            env=base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
            capture_output=True,
            text=True,
            timeout=120,
        )
        assert result.returncode == 0, f"cwd: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "cwd")
        assert_pekko_at(worktree, expected, "cwd")
        assert sorted(x.name for x in Path(root).iterdir()) == ["source", "stacked", "worktree"], (
            "cwd: pekko/ must only be created inside the worktree"
        )
        assert sorted(x.name for x in stacked.iterdir()) == [SCRIPT.name], (
            "cwd: pekko/ must not be created beside the worktree"
        )


def main() -> int:
    tests = [
        test_absent_clones_from_local_source,
        test_present_and_correct_is_noop,
        test_existing_pekko_with_other_head_is_not_removed,
        test_source_tag_pointing_elsewhere_fails_after_prepare,
        test_broken_empty_directory_is_rebuilt,
        test_broken_repo_without_head_is_rebuilt,
        test_broken_local_source_falls_back_to_upstream,
        test_missing_local_source_uses_shallow_upstream_clone,
        test_local_source_override_is_honoured,
        test_pekko_is_not_placed_in_the_callers_directory,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except Exception:  # 未実装（スクリプトが無い）や想定外は、失敗として記録する
            failures += 1
            print(f"FAIL {test.__name__}:")
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
