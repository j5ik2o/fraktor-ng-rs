#!/usr/bin/env python3
"""review_threads_gate.py の判定を、固定の応答で確かめる。

使い方:
    python3 tools/merge-gate/review_threads_gate_test.py

gh と git の呼び出しをスタブに置き換え、次の 3 通りを確かめる。
  1. 未解決が無い             exit 0、stdout の JSON（pr_number、unresolved_count=0）
  2. 未解決が有る（2 ページ） exit 1、stderr に件数と全スレッドの URL、cursor の受け渡し
  3. 件数を取得できない       exit 2、stderr に理由
解決済みや未解決が無いスレッドを数えないこと、作者（bot と人間の両方）で絞らないことも、
2 の応答に混ぜて確かめる。
"""
import importlib
import importlib.util
import io
import json
import re
import signal
import sys
import traceback
from pathlib import Path

GATE = Path(__file__).resolve().parent / "review_threads_gate.py"

# 欠陥のある実装はページングで無限ループし得る。1 テストの上限。
TEST_TIMEOUT_SECONDS = 10


class TestTimeout(Exception):
    pass


def _on_timeout(signum, frame):
    raise TestTimeout()

# 2 つのリポジトリ URL の書き方を、head と base で使い分ける。
BASE_URL = "https://github.com/example/owner.git"
HEAD_URL = "git@github.com:example/owner.git"
HEAD_BRANCH = "feature/merge-gate"
HEAD_SHA = "0123456789abcdef0123456789abcdef01234567"


def thread(index: int, resolved: bool, author: str) -> dict:
    url = f"https://github.com/example/owner/pull/42#discussion_r{index}"
    return {
        "isResolved": resolved,
        "comments": {"nodes": [{"url": url, "author": {"login": author}}]},
    }


def page(nodes: list[dict], has_next: bool, cursor: str | None) -> dict:
    return {
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "pageInfo": {"hasNextPage": has_next, "endCursor": cursor},
                        "nodes": nodes,
                    }
                }
            }
        }
    }


# 1 ページ目: 解決済み 1 件（数えない）、未解決 1 件（bot）、未解決 1 件（人間）。
# 2 ページ目: 未解決 1 件（別の bot）。合計の未解決は 3 件。
PAGE_1 = page(
    [
        thread(1, resolved=True, author="coderabbitai"),
        thread(2, resolved=False, author="coderabbitai"),
        thread(3, resolved=False, author="alice"),
    ],
    has_next=True,
    cursor="cursor-page-2",
)
PAGE_2 = page([thread(4, resolved=False, author="github-actions")], has_next=False, cursor=None)

# 解決済みしか無い 2 ページ分（0 件の経路）。
ZERO_1 = page([thread(10, resolved=True, author="coderabbitai")], has_next=True, cursor="z2")
ZERO_2 = page([thread(11, resolved=True, author="bob")], has_next=False, cursor=None)

GRAPHQL_ERRORS = {
    "data": {
        "repository": {
            "pullRequest": {
                "reviewThreads": {
                    "pageInfo": {"hasNextPage": False, "endCursor": None},
                    "nodes": [thread(7, resolved=False, author="alice")],
                }
            }
        }
    },
    "errors": [{"message": "Something went wrong while resolving review threads."}],
}


class GitGhStub:
    def __init__(self) -> None:
        self.calls: list[tuple[list[str], str | None]] = []

    def run(self, command: list[str], cwd: str | None = None) -> dict:
        self.calls.append((list(command), cwd))
        return self.dispatch(command, cwd)

    def dispatch(self, command: list[str], cwd: str | None) -> dict:
        raise AssertionError(f"unexpected command: {command}")


class GateRunner:
    """ファイルの gh 呼び出しだけを差し替え、main の標準入出力を捕まえる。"""

    def __init__(self, stub: GitGhStub) -> None:
        spec = importlib.util.spec_from_file_location("review_threads_gate", GATE)
        module = importlib.util.module_from_spec(spec)
        sys.modules["review_threads_gate"] = module
        spec.loader.exec_module(module)
        self.module = module
        module.run_command = stub.run
        self.stdout = io.StringIO()
        self.stderr = io.StringIO()

    def __enter__(self):
        self._stdout, self._stderr = sys.stdout, sys.stderr
        sys.stdout, sys.stderr = self.stdout, self.stderr
        return self

    def __exit__(self, *exc):
        sys.stdout, sys.stderr = self._stdout, self._stderr
        return False

    def run(self, argv: list[str]) -> int:
        return self.module.main(argv)


def gh_ok(stdout: str) -> dict:
    return {"status": 0, "stdout": stdout, "stderr": ""}


def gh_fail(stdout: str, stderr: str, status: int = 1) -> dict:
    return {"status": status, "stdout": stdout, "stderr": stderr}


def parse_stdout(text: str) -> dict:
    return json.loads(text)


def assert_unknown_counts_as_failure(result: int, stderr: str, label: str) -> None:
    assert result == 2, f"{label}: exit code must be 2, got {result}; stderr={stderr!r}"
    assert stderr.strip(), f"{label}: stderr must explain why the count could not be obtained"


def test_zero_unresolved_merge_allowed() -> None:
    class Fake(GitGhStub):
        def dispatch(self, command, cwd):
            if command[0] == "git":
                if command[1:3] == ["symbolic-ref", "--short"]:
                    return gh_ok(HEAD_BRANCH + "\n")
                if command[1:3] == ["rev-parse", "HEAD"]:
                    return gh_ok(HEAD_SHA + "\n")
                if command[1:3] == ["config", "--get"]:
                    return gh_ok(HEAD_URL + "\n")
            if command[0] == "gh":
                if command[1:2] == ["api"]:
                    cursor = next((t.split("=", 1)[1] for t in command if t.startswith("cursor=")), None)
                    return gh_ok(json.dumps(ZERO_2 if cursor == "z2" else ZERO_1))
                if command[1:3] == ["pr", "list"]:
                    return gh_ok(json.dumps([{"number": 42, "headRefOid": HEAD_SHA}]))
            return super().dispatch(command, cwd)

    stub = Fake()
    with GateRunner(stub) as runner:
        code = runner.run([])
        out = runner.stdout.getvalue()
        err = runner.stderr.getvalue()
    assert code == 0, f"zero case: expected exit 0, got {code}; stderr={err!r}"
    payload = parse_stdout(out)
    assert payload["pr_number"] == 42, payload
    assert payload["unresolved_count"] == 0, payload
    # 2 ページ目も取得する（解決済みだけでも続きがある限り数える）
    graphql_calls = [c for c in stub.calls if "api" in c[0][1:2]]
    assert len(graphql_calls) >= 2, f"zero case: must page through, got {len(graphql_calls)} call(s)"
    second = graphql_calls[1][0]
    assert any("cursor=z2" in token for token in second) or "z2" in " ".join(second), (
        f"zero case: second page must pass the previous endCursor z2, got {second}"
    )


def test_unresolved_aborts_with_count_and_urls() -> None:
    pages = {None: PAGE_1, "cursor-page-2": PAGE_2}

    class Fake(GitGhStub):
        def dispatch(self, command, cwd):
            if command[0] == "git":
                if command[1:3] == ["symbolic-ref", "--short"]:
                    return gh_ok(HEAD_BRANCH + "\n")
                if command[1:3] == ["rev-parse", "HEAD"]:
                    return gh_ok(HEAD_SHA + "\n")
                if command[1:3] == ["config", "--get"]:
                    return gh_ok(BASE_URL + "\n")
            if command[0] == "gh":
                if command[1:2] == ["api"]:
                    cursor = next((t.split("=", 1)[1] for t in command if t.startswith("cursor=")), None)
                    assert cursor in pages, f"unexpected cursor {cursor!r} in {command}"
                    return gh_ok(json.dumps(pages[cursor]))
                if command[1:3] == ["pr", "list"]:
                    return gh_ok(json.dumps([{"number": 42, "headRefOid": HEAD_SHA}]))
            return super().dispatch(command, cwd)

    stub = Fake()
    with GateRunner(stub) as runner:
        code = runner.run([])
        out = runner.stdout.getvalue()
        err = runner.stderr.getvalue()
    assert code == 1, f"unresolved case: expected exit 1, got {code}; stderr={err!r}"
    assert out.strip() == "", f"unresolved case: stdout must stay empty, got {out!r}"
    # 件数は URL と混同しないよう、件数の行を完全一致で取り出して照合する。URL は末尾が
    # discussion_r3 なので、単なる部分一致では誤った件数を見逃す。
    count_lines = [line for line in err.splitlines() if re.fullmatch(r"PR #42 の未解決レビュースレッド: \d+ 件", line)]
    assert len(count_lines) == 1, f"unresolved case: stderr must have exactly one count line; got {err!r}"
    assert count_lines[0] == "PR #42 の未解決レビュースレッド: 3 件", count_lines[0]
    # 作者で絞る誤実装（bot だけ・人間だけを数える）を検出する。r2・r4 は bot
    # （coderabbitai、github-actions）、r3 は人間（alice）なので、作者で絞ると URL が欠ける。
    for index in (2, 3, 4):
        url = f"https://github.com/example/owner/pull/42#discussion_r{index}"
        assert url in err, f"unresolved case: stderr must include {url}; got {err!r}"
    resolved_url = "https://github.com/example/owner/pull/42#discussion_r1"
    assert resolved_url not in err, f"resolved thread must not be reported; got {err!r}"


def test_unknown_count_aborts() -> None:
    # hasNextPage が true なのに endCursor が無い応答も、取得失敗として止める。
    # 空の cursor で先頭ページを引き直すと、無限に同じページを数え続ける。
    truncated = {
        "data": {
            "repository": {
                "pullRequest": {
                    "reviewThreads": {
                        "pageInfo": {"hasNextPage": True, "endCursor": None},
                        "nodes": [thread(5, resolved=False, author="alice")],
                    }
                }
            }
        }
    }
    cases = {
        "gh fails": gh_fail("", "error: HTTP 502"),
        "graphql errors": gh_ok(json.dumps(GRAPHQL_ERRORS)),
        "not json": gh_ok("<html>not json</html>"),
        "pullRequest null": gh_ok(json.dumps({"data": {"repository": {"pullRequest": None}}})),
        "missing endCursor": gh_ok(json.dumps(truncated)),
    }
    for label, response in cases.items():
        class Fake(GitGhStub):
            def dispatch(self, command, cwd, response=response):
                if command[0] == "git":
                    if command[1:3] == ["symbolic-ref", "--short"]:
                        return gh_ok(HEAD_BRANCH + "\n")
                    if command[1:3] == ["rev-parse", "HEAD"]:
                        return gh_ok(HEAD_SHA + "\n")
                    if command[1:3] == ["config", "--get"]:
                        return gh_ok(HEAD_URL + "\n")
                if command[0] == "gh":
                    if command[1:2] == ["api"]:
                        return response
                    if command[1:3] == ["pr", "list"]:
                        return gh_ok(json.dumps([{"number": 42, "headRefOid": HEAD_SHA}]))
                return super().dispatch(command, cwd)

        stub = Fake()
        with GateRunner(stub) as runner:
            code = runner.run([])
            err = runner.stderr.getvalue()
        assert_unknown_counts_as_failure(code, err, label)


def test_pr_cannot_be_identified() -> None:
    """PR を 1 件に特定できなければ、取得失敗として止める（取り違えない）。"""
    for label, listed in {
        "no match": [],
        "two matches": [{"number": 42, "headRefOid": HEAD_SHA}, {"number": 43, "headRefOid": HEAD_SHA}],
        # head が clone の HEAD と違う PR だけが返る。HEAD を照合せず先頭を採る誤実装は、
        # ここで別の PR を数えて merge に進む/別 PR を数える。照合する実装は 0 件一致で止まる。
        "head oid mismatch": [{"number": 99, "headRefOid": "f" * 40}],
    }.items():
        class Fake(GitGhStub):
            def dispatch(self, command, cwd, listed=listed):
                if command[0] == "git":
                    if command[1:3] == ["symbolic-ref", "--short"]:
                        return gh_ok(HEAD_BRANCH + "\n")
                    if command[1:3] == ["rev-parse", "HEAD"]:
                        return gh_ok(HEAD_SHA + "\n")
                    if command[1:3] == ["config", "--get"]:
                        return gh_ok(HEAD_URL + "\n")
                if command[0] == "gh" and command[1:3] == ["pr", "list"]:
                    return gh_ok(json.dumps(listed))
                if command[0] == "gh" and command[1:2] == ["api"]:
                    raise AssertionError(f"{label}: must not query reviewThreads without a single PR")
                return super().dispatch(command, cwd)

        stub = Fake()
        with GateRunner(stub) as runner:
            code = runner.run([])
            err = runner.stderr.getvalue()
        assert_unknown_counts_as_failure(code, err, label)


def test_explicit_repo_and_pr_are_honoured() -> None:
    """--repo と --pr が指定されたら、clone の情報を読まずにその PR を使う。"""
    class Fake(GitGhStub):
        def dispatch(self, command, cwd):
            if command[0] == "git":
                raise AssertionError("explicit --repo/--pr must not read the clone")
            if command[0] == "gh" and command[1:2] == ["api"]:
                cursor = next((t.split("=", 1)[1] for t in command if t.startswith("cursor=")), None)
                return gh_ok(json.dumps(ZERO_2 if cursor == "z2" else ZERO_1))
            return super().dispatch(command, cwd)

    stub = Fake()
    calls: list[list[str]] = []
    original = stub.run

    def recording(command, cwd=None):
        calls.append(list(command))
        return original(command, cwd)

    with GateRunner(stub) as runner:
        runner.module.run_command = recording
        code = runner.run(["--repo", "example/owner", "--pr", "42"])
        out = runner.stdout.getvalue()
    assert code == 0, f"explicit case: expected exit 0, got {code}"
    payload = json.loads(out)
    assert payload == {"pr_number": 42, "unresolved_count": 0}, payload
    joined = [" ".join(call) for call in calls]
    assert any("owner=example" in call or "name=owner" in call for call in joined), joined
    assert all("symbolic-ref" not in call for call in joined), joined

def main() -> int:
    tests = [
        test_zero_unresolved_merge_allowed,
        test_unresolved_aborts_with_count_and_urls,
        test_unknown_count_aborts,
        test_pr_cannot_be_identified,
        test_explicit_repo_and_pr_are_honoured,
    ]
    failures = 0
    for test in tests:
        # ページングの欠陥は無限ループになる。テスト自体が固まらないよう、各テストに上限を置く。
        signal.signal(signal.SIGALRM, _on_timeout)
        signal.alarm(TEST_TIMEOUT_SECONDS)
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except TestTimeout:
            failures += 1
            print(f"FAIL {test.__name__}: did not finish within {TEST_TIMEOUT_SECONDS}s")
        except Exception:  # 未実装（import できない）やスタブの想定外は、失敗として記録する
            failures += 1
            print(f"FAIL {test.__name__}:")
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
        finally:
            signal.alarm(0)
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
