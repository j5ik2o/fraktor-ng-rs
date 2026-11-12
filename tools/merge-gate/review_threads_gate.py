#!/usr/bin/env python3
"""対象 PR の未解決レビュースレッドを数える関門（TAKT の system step から固定コマンドで呼ばれる）。

対象 PR の reviewThreads を GitHub GraphQL で全ページ取得し、isResolved == false の
スレッドを数える。作者（bot・人間）では絞らない。判定は 1 か所で行い、終了コードと
stderr はその結果だけから作る。

終了コード:
  0  未解決 0 件。stdout に {"pr_number": N, "unresolved_count": 0} を出す
  1  未解決が 1 件以上。stderr に件数とスレッドの URL を出す
  2  件数を取得できない。stderr にその理由を出す

対象 PR は、clone の remote.origin.url（head リポジトリ）・HEAD ブランチ・HEAD の
コミットから特定する。merge_pr も clone の HEAD を基準にするため、この 3 つが同じ PR
を指すことを確かめる。`--repo OWNER/NAME --pr N` を渡した場合は clone を読まずその PR を使う。

TAKT は command に PATH/SHELL/TMPDIR などの限られた環境しか渡さない。HOME がないと
gh が認証情報を読めないため、子プロセスでは HOME が無ければログインユーザーのホームを補う。
"""
import json
import os
import pwd
import subprocess
import sys

REVIEW_THREADS_QUERY = """
query($owner: String!, $name: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          isResolved
          comments(first: 1) { nodes { url } }
        }
      }
    }
  }
}
"""


class GateError(Exception):
    """件数を取得できない理由を表す。"""


def _run(command: list[str], cwd: str | None = None) -> dict:
    env = dict(os.environ)
    if "HOME" not in env:
        # TAKT の command 環境には HOME が無い。gh の認証情報はログインユーザーの
        # ホームに置かれているため、復元しないと常に認証エラーになる。
        env["HOME"] = pwd.getpwuid(os.getuid()).pw_dir
    proc = subprocess.run(command, cwd=cwd, capture_output=True, text=True, env=env)
    return {"status": proc.returncode, "stdout": proc.stdout, "stderr": proc.stderr}


run_command = _run


def _parse_repo_slug(text: str) -> tuple[str, str]:
    value = text.strip()
    if value.endswith(".git"):
        value = value[:-4]
    if value.startswith("git@"):
        _, _, rest = value.partition(":")
    elif "://" in value:
        rest = value.split("://", 1)[1].partition("/")[2]
    else:
        rest = value
    owner, _, name = rest.partition("/")
    owner, name = owner.strip(), name.strip()
    if not owner or not name or "/" in name:
        raise GateError(f"リポジトリの URL を解析できません: {text!r}")
    return owner, name


def _git(*args: str) -> str:
    result = run_command(["git", *args])
    if result["status"] != 0:
        raise GateError(f"git {' '.join(args)} が失敗しました: {result['stderr'].strip()}")
    return result["stdout"].strip()


def resolve_target(argv: list[str]) -> tuple[str, str, int]:
    repo = pr = None
    index = 0
    while index < len(argv):
        argument = argv[index]
        if argument == "--repo":
            repo = argv[index + 1]
            index += 2
        elif argument == "--pr":
            pr = int(argv[index + 1])
            index += 2
        else:
            raise GateError(f"不明な引数です: {argument}")
    if repo is not None and pr is not None:
        owner, name = _parse_repo_slug(repo)
        return owner, name, pr
    if repo is not None or pr is not None:
        raise GateError("--repo と --pr は同時に指定してください")

    owner, name = _parse_repo_slug(_git("config", "--get", "remote.origin.url"))
    branch = _git("symbolic-ref", "--short", "HEAD")
    head_oid = _git("rev-parse", "HEAD")
    listing = run_command(
        ["gh", "pr", "list", "--repo", f"{owner}/{name}", "--head", branch, "--state", "open",
         "--json", "number,headRefOid"]
    )
    if listing["status"] != 0:
        raise GateError(f"gh pr list が失敗しました: {listing['stderr'].strip()}")
    try:
        candidates = json.loads(listing["stdout"])
    except ValueError as error:
        raise GateError(f"gh pr list の出力が JSON ではありません: {error}")
    matches = [candidate for candidate in candidates if candidate.get("headRefOid") == head_oid]
    if len(matches) != 1:
        raise GateError(
            f"HEAD {head_oid} に対応する open PR を 1 件に特定できません（{len(matches)} 件）"
        )
    return owner, name, int(matches[0]["number"])


def fetch_unresolved_thread_urls(owner: str, name: str, number: int) -> list[str]:
    unresolved: list[str] = []
    cursor = None
    while True:
        command = ["gh", "api", "graphql", "-f", f"query={REVIEW_THREADS_QUERY}",
                   "-F", f"owner={owner}", "-F", f"name={name}", "-F", f"number={number}"]
        if cursor is not None:
            command += ["-F", f"cursor={cursor}"]
        response = run_command(command)
        if response["status"] != 0:
            raise GateError(f"gh api graphql が失敗しました: {response['stderr'].strip()}")
        try:
            payload = json.loads(response["stdout"])
        except ValueError as error:
            raise GateError(f"GraphQL の応答が JSON ではありません: {error}")
        if payload.get("errors"):
            raise GateError(f"GraphQL の errors: {payload['errors']}")
        data = payload.get("data") or {}
        repository = data.get("repository") or {}
        pull_request = repository.get("pullRequest")
        if pull_request is None:
            raise GateError("GraphQL の応答に pullRequest がありません")
        threads = pull_request["reviewThreads"]
        for node in threads["nodes"]:
            if node.get("isResolved") is False:
                comments = (node.get("comments") or {}).get("nodes") or []
                unresolved.append(comments[0]["url"] if comments else "<URL 不明>")
        page_info = threads["pageInfo"]
        if not page_info.get("hasNextPage"):
            break
        cursor = page_info.get("endCursor")
        if not cursor:
            raise GateError("hasNextPage が true ですが endCursor がありません")
    return unresolved


def classify_outcome(argv: list[str]) -> tuple[int | None, str | None, list[str]]:
    try:
        owner, name, number = resolve_target(argv)
        unresolved = fetch_unresolved_thread_urls(owner, name, number)
    except GateError as error:
        return None, f"未解決件数を取得できませんでした: {error}", []
    return number, None, unresolved


def main(argv: list[str]) -> int:
    number, failure, unresolved = classify_outcome(argv)
    if failure is not None:
        print(failure, file=sys.stderr)
        return 2
    if unresolved:
        print(f"PR #{number} の未解決レビュースレッド: {len(unresolved)} 件", file=sys.stderr)
        for url in unresolved:
            print(url, file=sys.stderr)
        return 1
    print(f"PR #{number} の未解決レビュースレッド: 0 件", file=sys.stderr)
    print(json.dumps({"pr_number": number, "unresolved_count": 0}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
