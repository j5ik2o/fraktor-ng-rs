#!/usr/bin/env python3
"""pekko_baseline.py（基準の読み込み、verified_against の検査、現役経路の旧タグ検査）のテスト。

使い方:
    uv run --with pyyaml python3 ledger/tools/pekko_baseline_test.py

合成の基準ファイルと小さな git repo で規則を確かめ、最後に実際の基準ファイル・台帳・
現役の経路に対して check-ledger と check-refs が通ることを確かめる。
"""
import os
import subprocess
import sys
import tempfile
import traceback
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parents[1]
sys.path.insert(0, str(TOOLS))

import pekko_baseline  # noqa: E402

CURRENT = "vTEST-2"
OLD = "vTEST-1"


def git(*args: str, cwd: Path) -> str:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    return subprocess.run(
        ["git", "-c", "user.name=takt-test", "-c", "user.email=takt-test@example.invalid", *args],
        cwd=str(cwd), env=env, capture_output=True, text=True, check=True,
    ).stdout.strip()


def make_repo(path: Path, label: str) -> str:
    path.mkdir(parents=True, exist_ok=True)
    git("init", "-q", cwd=path)
    (path / "marker.txt").write_text(label, encoding="utf-8")
    git("add", "-A", cwd=path)
    git("commit", "-q", "-m", label, cwd=path)
    return git("rev-parse", "HEAD", cwd=path)


def baseline(current_commit: str = "a" * 40, old_commit: str = "b" * 40) -> pekko_baseline.Baseline:
    return pekko_baseline.Baseline(CURRENT, current_commit, None, None, {OLD: old_commit})


def write_baseline(path: Path, text: str) -> Path:
    path.write_text(text, encoding="utf-8")
    return path


def test_load_reads_current_and_historical() -> None:
    with tempfile.TemporaryDirectory() as root:
        path = write_baseline(Path(root) / "b.env", (
            "# comment\n\n"
            f"PEKKO_BASELINE_TAG={CURRENT}\nPEKKO_BASELINE_COMMIT={'a' * 40}\n"
            "PEKKO_BASELINE_DECIDED_AT=2026-11-25\n"
            f"PEKKO_HISTORICAL_BASELINES={OLD}:{'b' * 40}\n"
        ))
        b = pekko_baseline.load(path)
        assert (b.tag, b.commit, b.decided_at) == (CURRENT, "a" * 40, "2026-11-25"), b
        assert b.historical == {OLD: "b" * 40}, b
        assert b.known_tags() == {CURRENT, OLD}
        assert b.historical_tokens() == {OLD, "b" * 40}
        two = write_baseline(Path(root) / "two.env", (
            f"PEKKO_BASELINE_TAG={CURRENT}\nPEKKO_BASELINE_COMMIT={'a' * 40}\n"
            f"PEKKO_HISTORICAL_BASELINES={OLD}:{'b' * 40},vTEST-0:{'c' * 40}\n"
        ))
        assert pekko_baseline.load(two).historical == {OLD: "b" * 40, "vTEST-0": "c" * 40}


def test_load_rejects_malformed_files() -> None:
    cases = {
        "no tag": f"PEKKO_BASELINE_COMMIT={'a' * 40}\n",
        "short commit": f"PEKKO_BASELINE_TAG={CURRENT}\nPEKKO_BASELINE_COMMIT=abc\n",
        "bad historical": f"PEKKO_BASELINE_TAG={CURRENT}\nPEKKO_BASELINE_COMMIT={'a' * 40}\n"
                          f"PEKKO_HISTORICAL_BASELINES={OLD}\n",
        "current is historical": f"PEKKO_BASELINE_TAG={CURRENT}\nPEKKO_BASELINE_COMMIT={'a' * 40}\n"
                                 f"PEKKO_HISTORICAL_BASELINES={CURRENT}:{'b' * 40}\n",
        "not key=value": f"PEKKO_BASELINE_TAG {CURRENT}\n",
    }
    with tempfile.TemporaryDirectory() as root:
        for label, text in cases.items():
            path = write_baseline(Path(root) / "b.env", text)
            try:
                pekko_baseline.load(path)
            except pekko_baseline.BaselineError:
                continue
            raise AssertionError(f"{label}: load must reject {text!r}")


def test_source_tag_accepts_only_the_current_commit() -> None:
    with tempfile.TemporaryDirectory() as root:
        pekko = Path(root) / "pekko"
        head = make_repo(pekko, "current")
        assert pekko_baseline.source_tag(baseline(current_commit=head), pekko) == CURRENT
        try:
            pekko_baseline.source_tag(baseline(old_commit=head), pekko)
        except pekko_baseline.BaselineError as error:
            assert OLD in str(error) and head in str(error), error
        else:
            raise AssertionError("a historical baseline checkout must not resolve to the current tag")


def test_source_tag_rejects_local_changes_and_keeps_them() -> None:
    """HEAD が基準 commit でも、変更・未追跡・印付きのファイルがあれば拒否し、中身は変えない。"""
    changes = {
        "modified": lambda repo: (repo / "marker.txt").write_text("older baseline content", encoding="utf-8"),
        "staged": lambda repo: ((repo / "marker.txt").write_text("staged", encoding="utf-8"),
                                git("add", "marker.txt", cwd=repo)),
        "untracked": lambda repo: (repo / "Extra.java").write_text("x", encoding="utf-8"),
        "assume-unchanged": lambda repo: (git("update-index", "--assume-unchanged", "marker.txt", cwd=repo),
                                          (repo / "marker.txt").write_text("hidden", encoding="utf-8")),
        "skip-worktree": lambda repo: (git("update-index", "--skip-worktree", "marker.txt", cwd=repo),
                                       (repo / "marker.txt").write_text("hidden", encoding="utf-8")),
    }
    for label, change in changes.items():
        with tempfile.TemporaryDirectory() as root:
            pekko = Path(root) / "pekko"
            head = make_repo(pekko, "current")
            change(pekko)
            before = {p.name: p.read_bytes() for p in pekko.iterdir() if p.is_file()}
            try:
                pekko_baseline.source_tag(baseline(current_commit=head), pekko)
            except pekko_baseline.BaselineError:
                pass
            else:
                raise AssertionError(f"{label}: a changed source must not resolve to the current tag")
            assert {p.name: p.read_bytes() for p in pekko.iterdir() if p.is_file()} == before, label


def test_source_tag_rejects_a_source_git_cannot_inspect() -> None:
    """index が壊れて git status が失敗したら拒否する（変更なしと見なさない）。原典は残す。"""
    with tempfile.TemporaryDirectory() as root:
        pekko = Path(root) / "pekko"
        head = make_repo(pekko, "current")
        index = pekko / ".git" / "index"
        index.write_bytes(b"not an index")
        try:
            pekko_baseline.source_tag(baseline(current_commit=head), pekko)
        except pekko_baseline.BaselineError as error:
            assert "status" in str(error) or "ls-files" in str(error), error
        else:
            raise AssertionError("a source git cannot inspect must not resolve to the current tag")
        assert index.read_bytes() == b"not an index" and (pekko / "marker.txt").exists()


def test_source_root_follows_pekko_original_root() -> None:
    old = os.environ.pop("PEKKO_ORIGINAL_ROOT", None)
    try:
        assert pekko_baseline.source_root() == Path("pekko")
        os.environ["PEKKO_ORIGINAL_ROOT"] = "/somewhere/pekko"
        assert pekko_baseline.source_root() == Path("/somewhere/pekko")
    finally:
        os.environ.pop("PEKKO_ORIGINAL_ROOT", None)
        if old is not None:
            os.environ["PEKKO_ORIGINAL_ROOT"] = old


def test_source_tag_rejects_missing_and_nested_directories() -> None:
    with tempfile.TemporaryDirectory() as root:
        outer = Path(root) / "outer"
        head = make_repo(outer, "outer")
        nested = outer / "pekko"
        nested.mkdir()
        for path in (Path(root) / "absent", nested):
            try:
                pekko_baseline.source_tag(baseline(current_commit=head), path)
            except pekko_baseline.BaselineError:
                continue
            raise AssertionError(f"{path}: must not resolve (absent or inside another repo)")


def test_check_nodes_rules() -> None:
    b = baseline()
    ok = [
        {"id": "a", "status": "green", "verified_against": OLD},
        {"id": "b", "status": "green", "verified_against": CURRENT},
        {"id": "c", "status": "do_not_port", "verified_against": OLD},
        {"id": "d", "status": "not_built", "verified_against": CURRENT},
        {"id": "e", "status": "unported"},
        {"id": "f", "status": "red", "verified_against": None},
    ]
    assert pekko_baseline.check_nodes(ok, b) == []
    bad = [
        {"id": "g", "status": "green"},
        {"id": "h", "status": "do_not_port", "verified_against": None},
        {"id": "i", "status": "not_built", "verified_against": "v9.9.9"},
        {"id": "j", "status": "unported", "verified_against": CURRENT},
        {"id": "k", "status": "green", "verified_against": 2},
    ]
    errors = pekko_baseline.check_nodes(bad, b)
    assert [e.split(":")[0] for e in errors] == ["g", "h", "i", "j", "k"], errors


def test_summarize_separates_current_from_unverified() -> None:
    nodes = [
        {"id": "a", "status": "green", "verified_against": OLD},
        {"id": "b", "status": "green", "verified_against": CURRENT},
        {"id": "c", "status": "not_built", "verified_against": OLD},
        {"id": "d", "status": "unported"},
    ]
    summary = pekko_baseline.summarize(nodes, baseline())
    assert [n["id"] for n in summary["unverified"]] == ["a", "c"], summary
    assert summary["by_tag"][("green", CURRENT)] == 1 and summary["by_tag"][("green", OLD)] == 1


def test_find_stale_refs_allows_only_history_directory_paths() -> None:
    with tempfile.TemporaryDirectory() as root:
        root = Path(root)
        git("init", "-q", cwd=root)
        # TAKT の run の記録は git が無視する。旧タグが残っていても現役の参照ではない
        (root / ".takt").mkdir()
        (root / ".takt" / ".gitignore").write_text("runs/\n", encoding="utf-8")
        report = root / ".takt" / "runs" / "20261120-000000" / "reports" / "triage.md"
        report.parent.mkdir(parents=True)
        report.write_text(f"原典: Pekko {OLD}\n", encoding="utf-8")
        facet = root / ".takt" / "facets" / "x.md"
        facet.parent.mkdir(parents=True)
        facet.write_text(
            f"原典は Pekko {CURRENT}\n"
            f"語彙の正は docs/baseline/pekko-{OLD}/supplement-recount.md\n"
            f"原典は Pekko {OLD}\n"
            f"commit {'b' * 40}\n",
            encoding="utf-8",
        )
        history_tool = root / "ledger" / "tools" / "convert_baseline.py"
        history_tool.parent.mkdir(parents=True)
        history_tool.write_text(f"# {OLD}\n", encoding="utf-8")
        (root / "docs").mkdir()
        (root / "docs" / "old-goal.md").write_text(f"{OLD}\n", encoding="utf-8")
        hits = pekko_baseline.find_stale_refs(root, baseline())
        assert [h.split(": ", 1)[0] for h in hits] == [".takt/facets/x.md:3", ".takt/facets/x.md:4"], hits
        # 同じ行を現役の facet に追跡ファイルとして置けば検出する
        git("add", ".takt/facets/x.md", cwd=root)
        tracked = root / ".takt" / "facets" / "y.md"
        tracked.write_text(f"原典: Pekko {OLD}\n", encoding="utf-8")
        git("add", ".takt/facets/y.md", cwd=root)
        hits = pekko_baseline.find_stale_refs(root, baseline())
        assert ".takt/facets/y.md:1" in [h.split(": ", 1)[0] for h in hits], hits


def run_cli(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(TOOLS / "pekko_baseline.py"), *args],
                          cwd=str(REPO), capture_output=True, text=True, check=False)


def test_real_baseline_file_ledger_and_active_paths() -> None:
    real = pekko_baseline.load()
    assert real.tag not in real.historical, real
    ledger = run_cli("check-ledger", "--ledger", str(REPO / "ledger" / "nodes.yaml"))
    assert ledger.returncode == 0, (ledger.stdout, ledger.stderr)
    assert "verified_against: OK" in ledger.stdout, ledger.stdout
    refs = run_cli("check-refs")
    assert refs.returncode == 0, (refs.stdout, refs.stderr)


def main() -> int:
    tests = [
        test_load_reads_current_and_historical,
        test_load_rejects_malformed_files,
        test_source_tag_accepts_only_the_current_commit,
        test_source_tag_rejects_local_changes_and_keeps_them,
        test_source_tag_rejects_a_source_git_cannot_inspect,
        test_source_root_follows_pekko_original_root,
        test_source_tag_rejects_missing_and_nested_directories,
        test_check_nodes_rules,
        test_summarize_separates_current_from_unverified,
        test_find_stale_refs_allows_only_history_directory_paths,
        test_real_baseline_file_ledger_and_active_paths,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except Exception:
            failures += 1
            print(f"FAIL {test.__name__}:")
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
