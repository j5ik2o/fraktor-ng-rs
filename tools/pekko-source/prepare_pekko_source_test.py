#!/usr/bin/env python3
"""準備スクリプトの判定を、テスト用の小さな git repo で確かめる。

使い方:
    uv run --with pyyaml python3 tools/pekko-source/prepare_pekko_source_test.py

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
import time
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


def clean_env(source: dict[str, str] | None = None) -> dict[str, str]:
    """GIT_* を除いた環境を返す。

    Git の hook から起動されると GIT_DIR などが設定されている。そのまま git を
    呼ぶと、一時ディレクトリではなく呼び出し元の repo を操作してしまう。
    """
    source = os.environ if source is None else source
    return {k: v for k, v in source.items() if not k.startswith("GIT_")}


def git(*args: str, cwd: str | Path, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *GIT_IDENTITY, *args],
        cwd=str(cwd),
        env=clean_env(),
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


def make_distinct_repo(path: str | Path, label: str) -> str:
    """make_repo とは異なる commit を持つ repo を作る。

    make_repo は同じ identity とメッセージを使うため commit が決定的になり、
    別ディレクトリでも同じ hash になる。呼び出し元 repo を別 commit にして、
    GIT_DIR が漏れたときに hash の一致で見逃さないようにする。
    """
    make_repo(path)
    extra = Path(path) / "caller-marker.txt"
    extra.write_text(f"{label}\n", encoding="utf-8")
    git("add", extra.name, cwd=path)
    git("commit", "-q", "-m", label, cwd=path)
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
    env = clean_env({k: v for k, v in os.environ.items() if k != "HOME"})
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


FAKE_NPM = """#!/usr/bin/env bash
# npm ci を模す。呼び出しを記録し、作業ツリーに node_modules を作る。
# 実 npm と同じく、node_modules/.package-lock.json は package-lock.json と
# 同一内容ではなく、aislop の実行物（.bin/aislop）を作る。
set -euo pipefail
echo "npm $*" >> "$FAKE_NPM_LOG"
case "${1:-}" in
  ci)
    rm -rf node_modules
    mkdir -p node_modules/.bin
    echo '{"resolved":"fake"}' > node_modules/.package-lock.json
    printf '#!/usr/bin/env node\\n' > node_modules/.bin/aislop
    chmod +x node_modules/.bin/aislop
    ;;
esac
exit 0
"""


def install_fake_npm(worktree: str | Path) -> Path:
    """node_modules を持たない作業ツリーに、npm を差し替えるための bin と npm ci の前提を作る。"""
    worktree = Path(worktree)
    (worktree / "package.json").write_text('{"devDependencies": {"aislop": "0.20.1"}}\n',
                                            encoding="utf-8")
    (worktree / "package-lock.json").write_text('{"lockfileVersion": 3, "packages": {}}\n',
                                                encoding="utf-8")
    bin_dir = worktree / ".fake-bin"
    bin_dir.mkdir(exist_ok=True)
    npm = bin_dir / "npm"
    npm.write_text(FAKE_NPM, encoding="utf-8")
    npm.chmod(0o755)
    return bin_dir


def prepare_env_with_npm(root: str | Path, source: str | Path, expected: str,
                         worktree: str | Path) -> tuple[dict[str, str], Path]:
    """base_env に、偽の npm を先頭へ足した PATH と、呼び出し記録のパスを加える。"""
    log = Path(root) / "npm-calls.log"
    log.write_text("", encoding="utf-8")
    bin_dir = install_fake_npm(worktree)
    env = base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected)
    env["PATH"] = f"{bin_dir}{os.pathsep}{env.get('PATH', '')}"
    env["FAKE_NPM_LOG"] = str(log)
    return env, log


def assert_node_modules_prepared(worktree: str | Path, label: str) -> None:
    aislop = Path(worktree) / "node_modules" / ".bin" / "aislop"
    assert aislop.exists(), f"{label}: prepare must leave node_modules/.bin/aislop for the aislop gate"


def test_prepare_installs_node_modules_so_aislop_can_run() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        env, log = prepare_env_with_npm(root, source, expected, worktree)

        result = run_script(worktree, env)
        assert result.returncode == 0, f"npm ci: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "npm ci")
        calls = log.read_text(encoding="utf-8").splitlines()
        assert any(call.startswith("npm ci") for call in calls), (
            f"npm ci: prepare must run npm ci to prepare node_modules, got calls={calls!r}"
        )
        assert_node_modules_prepared(worktree, "npm ci")


def test_prepare_still_installs_node_modules_when_pekko_is_already_prepared() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        env, log = prepare_env_with_npm(root, source, expected, worktree)

        # 1 回目で pekko/ と node_modules を用意する。
        first = run_script(worktree, env)
        assert first.returncode == 0, f"npm ci: first run expected 0, got {first.returncode}; stderr={first.stderr!r}"
        assert_silent_stdout(first, "npm ci first")
        assert_node_modules_prepared(worktree, "npm ci first")

        # node_modules を消して、pekko/ だけが揃った再実行を模す。pekko/ は clone では
        # なく正しさの照合で抜ける経路になるが、node_modules の用意は動き続ける必要がある。
        shutil.rmtree(Path(worktree) / "node_modules")
        log.write_text("", encoding="utf-8")

        second = run_script(worktree, env)
        assert second.returncode == 0, (
            f"npm ci: second run expected 0, got {second.returncode}; stderr={second.stderr!r}"
        )
        assert_silent_stdout(second, "npm ci second")
        assert_node_modules_prepared(worktree, "npm ci second")

        # node_modules が揃っている 3 回目は npm ci を再実行しない（冪等）。
        log.write_text("", encoding="utf-8")
        third = run_script(worktree, env)
        assert third.returncode == 0, (
            f"npm ci: third run expected 0, got {third.returncode}; stderr={third.stderr!r}"
        )
        assert_silent_stdout(third, "npm ci third")
        assert log.read_text(encoding="utf-8").strip() == "", (
            f"npm ci: an up-to-date node_modules must not trigger another npm ci, got {log.read_text(encoding='utf-8')!r}"
        )


def test_prepare_reinstalls_when_the_lockfile_is_newer_than_node_modules() -> None:
    # package-lock.json が node_modules より新しい（依存が変わった）ときは npm ci をやり直す。
    # mtime 比較の向きを逆にした実装や、片方のファイルだけを見る実装を拒否する。
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        env, log = prepare_env_with_npm(root, source, expected, worktree)

        first = run_script(worktree, env)
        assert first.returncode == 0, f"fresh install: expected 0, got {first.returncode}; stderr={first.stderr!r}"
        assert_node_modules_prepared(worktree, "fresh install")

        # lockfile を node_modules/.bin/aislop より新しくする。
        time.sleep(1.1)
        os.utime(Path(worktree) / "package-lock.json", None)
        log.write_text("", encoding="utf-8")

        second = run_script(worktree, env)
        assert second.returncode == 0, (
            f"lock newer: expected 0, got {second.returncode}; stderr={second.stderr!r}"
        )
        assert_silent_stdout(second, "lock newer")
        assert any(call.startswith("npm ci") for call in log.read_text(encoding="utf-8").splitlines()), (
            "lock newer: a changed package-lock.json must trigger npm ci"
        )


def test_prepare_reinstalls_when_aislop_binary_is_a_broken_symlink() -> None:
    # npm の .bin/aislop は symlink。リンク先が消えている（壊れている）なら
    # 実行物が無いのと同じなので npm ci をやり直す。[ -e ] ではなく [ -x ] で
    # 判定しないと、壊れたリンクを「用意済み」と誤認して aislop が動かなくなる。
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        worktree = make_worktree(root)
        env, log = prepare_env_with_npm(root, source, expected, worktree)

        first = run_script(worktree, env)
        assert first.returncode == 0, f"fresh install: expected 0, got {first.returncode}; stderr={first.stderr!r}"
        assert_node_modules_prepared(worktree, "fresh install")

        # .bin/aislop を、存在しないリンク先を指す symlink に置き換える。
        aislop = Path(worktree) / "node_modules" / ".bin" / "aislop"
        aislop.unlink()
        aislop.symlink_to("../aislop/dist/cli.js")
        assert not aislop.exists(), "the symlink must be broken for this case"
        log.write_text("", encoding="utf-8")

        second = run_script(worktree, env)
        assert second.returncode == 0, (
            f"broken symlink: expected 0, got {second.returncode}; stderr={second.stderr!r}"
        )
        assert any(call.startswith("npm ci") for call in log.read_text(encoding="utf-8").splitlines()), (
            "broken symlink: a dangling .bin/aislop must trigger npm ci"
        )
        assert_node_modules_prepared(worktree, "broken symlink")


def test_prepare_without_a_lockfile_does_not_run_npm() -> None:
    # package-lock.json が無い作業ツリー（package.json を置いていない）では
    # node_modules を用意する必要が無い。npm を呼ばずに後続へ進む。
    with tempfile.TemporaryDirectory() as root:
        worktree = Path(root) / "worktree"
        os.makedirs(worktree)
        git("init", "-q", cwd=worktree)
        # pekko/ を期待コミットで用意し、npm 段階だけを観測する。
        source = Path(root) / "source"
        expected = make_source(source)
        env, log = prepare_env_with_npm(root, source, expected, worktree)
        (worktree / "package-lock.json").unlink()
        (worktree / "package.json").unlink()

        result = run_script(worktree, env)
        assert result.returncode == 0, f"no lockfile: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        assert_silent_stdout(result, "no lockfile")
        assert log.read_text(encoding="utf-8").strip() == "", (
            f"no lockfile: npm must not run when there is nothing to install, got {log.read_text(encoding='utf-8')!r}"
        )


def test_ci_runs_check_aislop_after_installing_dependencies() -> None:
    # ステップ定義の順序を観測する。全文の文字列位置ではなく、`jobs.check.steps` の
    # 並びで「npm ci を実行するステップ」が「npm run check:aislop を実行するステップ」
    # より前にあることを確かめる。表示名（name）ではなく実行コマンド（run）を見る。
    import yaml

    ci = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "ci.yml"
    workflow = yaml.safe_load(ci.read_text(encoding="utf-8"))
    steps = workflow["jobs"]["check"]["steps"]

    def runs(command: str) -> list[int]:
        # ステップの run を実行コマンドとして扱い、コマンド行の先頭一致で探す。
        # 複数行の run は行ごとに見る。
        found = []
        for index, step in enumerate(steps):
            script = step.get("run")
            if not isinstance(script, str):
                continue
            if any(line.strip().startswith(command) for line in script.splitlines()):
                found.append(index)
        return found

    install = runs("npm ci")
    check = runs("npm run check:aislop")
    assert install, "ci.yml has no step that runs 'npm ci'"
    assert check, "ci.yml has no step that runs 'npm run check:aislop'"
    assert install[0] < check[0], (
        f"ci.yml runs 'npm run check:aislop' (step {check[0]}) before 'npm ci' (step {install[0]})"
    )


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
        # この実行で作った pekko/ を残すと、原典を直しても既存 repo として拒否され、
        # 自動準備に戻れなくなる。
        assert not (Path(worktree) / "pekko").exists(), (
            "wrong tag: the pekko/ cloned by this run must be removed when verification fails"
        )

        # 原典を直して再実行すると、自動準備で成功する。
        git("tag", "-f", "v1.7.1", cwd=source)
        result2 = run_script(
            worktree,
            base_env(PEKKO_SOURCE_LOCAL_REPO=str(source), PEKKO_SOURCE_EXPECTED_COMMIT=expected),
        )
        assert result2.returncode == 0, (
            f"wrong tag: after fixing the source a re-run must succeed, got {result2.returncode}; stderr={result2.stderr!r}"
        )
        assert_silent_stdout(result2, "wrong tag re-run")
        assert_pekko_at(worktree, expected, "wrong tag re-run")


def test_inherited_git_dir_does_not_leak_into_test_repos() -> None:
    with tempfile.TemporaryDirectory() as root:
        caller = Path(root) / "caller"
        caller_head = make_distinct_repo(caller, "caller")
        saved = {k: os.environ[k] for k in ("GIT_DIR", "GIT_INDEX_FILE") if k in os.environ}
        os.environ["GIT_DIR"] = str(caller / ".git")
        os.environ["GIT_INDEX_FILE"] = str(caller / ".git" / "index")
        try:
            fresh = Path(root) / "fresh"
            created = make_repo(fresh)
        finally:
            for key in ("GIT_DIR", "GIT_INDEX_FILE"):
                os.environ.pop(key, None)
            os.environ.update(saved)

        assert git("rev-parse", "HEAD", cwd=caller) == caller_head, (
            "inherited GIT_DIR: the caller repo must keep its HEAD"
        )
        assert git("for-each-ref", "--format=%(refname)", cwd=caller).count("\n") == 0, (
            "inherited GIT_DIR: the caller repo must not gain refs"
        )
        assert created and created != caller_head, (
            "inherited GIT_DIR: make_repo must create a commit in the new repo, not the caller repo"
        )
        assert git("rev-parse", "HEAD", cwd=fresh) == created, (
            "inherited GIT_DIR: the new repo must hold the commit it reported"
        )


def test_inherited_git_dir_does_not_affect_script() -> None:
    with tempfile.TemporaryDirectory() as root:
        source = Path(root) / "source"
        expected = make_source(source)
        caller = Path(root) / "caller"
        caller_head = make_distinct_repo(caller, "caller")
        worktree = make_worktree(root)

        result = run_script(
            worktree,
            base_env(
                PEKKO_SOURCE_LOCAL_REPO=str(source),
                PEKKO_SOURCE_EXPECTED_COMMIT=expected,
                GIT_DIR=str(caller / ".git"),
            ),
        )
        assert result.returncode == 0, (
            f"inherited GIT_DIR: expected 0, got {result.returncode}; stderr={result.stderr!r}"
        )
        assert_silent_stdout(result, "inherited GIT_DIR")
        assert_pekko_at(worktree, expected, "inherited GIT_DIR")
        assert git("rev-parse", "HEAD", cwd=caller) == caller_head, (
            "inherited GIT_DIR: the script must not change the repo GIT_DIR points at"
        )


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
        test_inherited_git_dir_does_not_leak_into_test_repos,
        test_inherited_git_dir_does_not_affect_script,
        test_broken_empty_directory_is_rebuilt,
        test_broken_repo_without_head_is_rebuilt,
        test_broken_local_source_falls_back_to_upstream,
        test_missing_local_source_uses_shallow_upstream_clone,
        test_local_source_override_is_honoured,
        test_pekko_is_not_placed_in_the_callers_directory,
        test_prepare_installs_node_modules_so_aislop_can_run,
        test_prepare_still_installs_node_modules_when_pekko_is_already_prepared,
        test_prepare_reinstalls_when_the_lockfile_is_newer_than_node_modules,
        test_prepare_reinstalls_when_aislop_binary_is_a_broken_symlink,
        test_prepare_without_a_lockfile_does_not_run_npm,
        test_ci_runs_check_aislop_after_installing_dependencies,
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
