#!/usr/bin/env python3
"""verified_against を現行タグへ揃える道具（ledger/tools/reverify_against.py）の回帰テスト。

確かめること:
- 変えるのは対象ノードの verified_against の値だけで、行数・ほかの欄・行頭の注記は変わらない
- 欄が無いノードには、ノードの末尾（`goal` の後）に足す。`retired` が `nodes` より前にある台帳でも、
  退役したノードを書き換えない
- 判定済みでない status（unported など）と、nodes に無い id は拒否し、台帳を書き換えない
- 現行タグでない値（過去のタグ）は拒否し、台帳を書き換えない（照合済みのノードを未照合に戻さない）
- plan は現行タグで未照合の判定済みノードだけを出す
- report は原典を検査してから表を作る（未準備の `pekko/` では数えずに失敗する）。判定済みの
  全件を既定で出し、`--unverified-only` で未照合だけに絞る
- PyYAML なし（`python3 -S`）でも apply と plan が動く（report だけが PyYAML を使う）

使い方:
    uv run --with pyyaml python3 ledger/tools/reverify_against_test.py
"""
import subprocess
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import pekko_baseline  # noqa: E402
import reverify_against  # noqa: E402

# タグは基準ファイルから取る（現役の経路に過去のタグや現行タグを固定して書かない。check-refs の規則）。
# このテストは、その値を入力の文字列として使うだけで、過去の原典を読まない。
BASELINE = pekko_baseline.load()
CURRENT = BASELINE.tag
PAST = sorted(BASELINE.historical)[0]

NODE = """- id: {id}
  name: m:{name}
  kind: test
  pekko:
    module: m
    class: {name}
    test_cases_approx: 1
  status: {status}
  triage:
    kind: {kind}
    reason: null
  needs:
    testkit: []
    premise_flags: []
  depends_on_impl:
    modules: []
    coverage: approx
  cases:
    total: null
    ported: null
    passing: null
    excluded: []
  rust:
    crate: null
    test_file: null
  goal: null{tag}
"""

RETIRED = """- id: retired/Old.scala
  name: m:Old
  kind: test
  pekko:
    module: m
    class: Old
    test_cases_approx: 1
  status: green
  triage:
    kind: pending
    reason: null
  needs:
    testkit: []
    premise_flags: []
  depends_on_impl:
    modules: []
    coverage: approx
  cases:
    total: 1
    ported: 1
    passing: 1
    excluded: []
  rust:
    crate: c
    test_file: t.rs
  goal: null
  verified_against: {past}
  retired:
    at: 2026-11-25
    reason: "現行の原典にファイルが無い"
    successor: m/New.scala
"""


def ledger(*, retired_first: bool = False, tag: str | None = None) -> str:
    """2 ノード（do_not_port と unported）の台帳。tag=False で verified_against の欄を省く。"""
    tag = PAST if tag is None else tag
    tag_line = "" if tag is False else f"\n  verified_against: {tag}"
    body = (
        NODE.format(id="m/A.scala", name="A", status="do_not_port", kind="do_not_port", tag=tag_line)
        + NODE.format(id="m/B.scala", name="B", status="unported", kind="pending", tag="")
    )
    head = "# 台帳の見出し（行頭の注記）\n\n"
    retired = RETIRED.format(past=PAST)
    if retired_first:
        return head + "retired:\n" + retired + "\n" + "nodes:\n" + body
    return head + "nodes:\n" + body + "\n" + "# 退役したノード\nretired:\n" + retired


def write_ledger(root: Path, text: str) -> Path:
    path = root / "nodes.yaml"
    path.write_text(text, encoding="utf-8")
    return path


def write_ids(root: Path, ids: list[str]) -> Path:
    path = root / "ids.txt"
    path.write_text("# 対象\n" + "\n".join(ids) + "\n", encoding="utf-8")
    return path


def tool(root: Path, *args: str, env: dict | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOLS / "reverify_against.py"), *args],
        capture_output=True, text=True, check=False, env=env,
    )


def apply(root: Path, ids: list[str], *extra: str, env: dict | None = None) -> subprocess.CompletedProcess:
    return tool(root, "apply", "--ledger", str(root / "nodes.yaml"),
                "--ids", str(write_ids(root, ids)), *extra, env=env)


def make_source(root: Path, files: dict[str, str]) -> tuple[Path, str]:
    """`pekko/` の代わりになる git repo を作り、(パス, HEAD の commit) を返す。"""
    pekko = root / "pekko"
    pekko.mkdir()
    for name, text in files.items():
        path = pekko / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def git(*args: str) -> str:
        proc = subprocess.run(
            ["git", "-C", str(pekko), "-c", "user.name=t", "-c", "user.email=t@example.com", *args],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (args, proc.stderr)
        return proc.stdout.strip()

    git("init", "-q")
    git("add", "-A")
    git("commit", "-q", "-m", "source")
    return pekko, git("rev-parse", "--verify", "HEAD^{commit}")


def write_baseline(root: Path, commit: str) -> Path:
    """作業ツリーの原典を現行の基準にする、テスト用の基準ファイルを書く。"""
    path = root / "baseline.env"
    path.write_text(
        f"PEKKO_BASELINE_TAG={CURRENT}\n"
        f"PEKKO_BASELINE_COMMIT={commit}\n"
        f"PEKKO_HISTORICAL_BASELINES={PAST}:{commit}\n",
        encoding="utf-8",
    )
    return path


def test_apply_changes_only_the_value():
    """do_not_port のノードの値だけが変わり、行数もほかの行もそのまま残る。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        expected = before.replace(
            f"  verified_against: {PAST}\n- id: m/B.scala",
            f"  verified_against: {CURRENT}\n- id: m/B.scala", 1,
        )
        assert after == expected, after
        assert after.count(f"verified_against: {PAST}") == 1, after
        print("1. do_not_port の値だけを書き換える")


def test_apply_adds_the_field_after_goal():
    """欄が無いノードには、`goal` の後（ノードの末尾）に足す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger(tag=False)
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        assert after == before.replace(
            "  goal: null\n- id: m/B.scala",
            f"  goal: null\n  verified_against: {CURRENT}\n- id: m/B.scala",
        ), after
        print("2. 欄が無ければ goal の後に足す")


def test_apply_refuses_a_status_that_is_not_judged():
    """unported のノードは拒否し、台帳を 1 バイトも書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/B.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "unported" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("3. unported は拒否する")


def test_apply_refuses_an_id_that_is_not_in_nodes():
    """nodes に無い id（退役したノードの id）は拒否し、retired を書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["retired/Old.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "nodes に無い" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("4. retired の id は拒否する")


def test_apply_refuses_a_tag_that_is_not_current():
    """過去のタグは拒否し、照合済みのノードを未照合に戻さない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger(tag=CURRENT)
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"], "--tag", PAST)
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "現行の基準タグ" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("5. 現行タグでない --tag は拒否する")


def test_apply_keeps_retired_first_and_comments():
    """`retired` が `nodes` より前にある台帳でも、注記と退役したノードをそのまま残す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger(retired_first=True)
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        # retired の節（nodes: より前）と、ノードの見出しの注記は 1 バイトも変わらない。
        assert after[:after.index("nodes:")] == before[:before.index("nodes:")], after[:200]
        # 変わるのは対象ノードの 1 行だけで、退役したノードの verified_against は過去のタグのまま。
        assert after == before.replace(
            f"  verified_against: {PAST}\n- id: m/B.scala",
            f"  verified_against: {CURRENT}\n- id: m/B.scala",
        ), after
        assert f"verified_against: {PAST}" in after, after
        print("6. retired が先頭でも注記と退役を残す")


def test_apply_is_idempotent_and_reports_no_change():
    """既に現行タグのノードは書き換えず、変更 0 件として成功する。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        path = write_ledger(root, ledger())
        assert apply(root, ["m/A.scala"]).returncode == 0
        once = path.read_text(encoding="utf-8")
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更した: 0 件" in proc.stdout, proc.stdout
        assert path.read_text(encoding="utf-8") == once
        print("7. 再実行しても変わらない")


def test_apply_refuses_duplicate_ids_and_writes_nothing():
    """--ids の重複は拒否する（同じノードを 2 回書かない）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala", "m/A.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "重複" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("8. --ids の重複は拒否する")


def test_dry_run_writes_nothing():
    """--dry-run は変更件数だけを出し、台帳を書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"], "--dry-run")
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更する: 1 件" in proc.stdout, proc.stdout
        assert path.read_text(encoding="utf-8") == before
        print("9. --dry-run は書かない")


def test_plan_lists_judged_nodes_with_another_tag():
    """plan は判定済みで現行タグと違うノードだけを、render せずに id で出す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_ledger(root, ledger(retired_first=True))
        proc = tool(root, "plan", "--ledger", str(root / "nodes.yaml"))
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert proc.stdout.splitlines() == ["m/A.scala"], proc.stdout
        print("10. plan は判定済みの未照合だけを出す")


def test_report_table_marks_missing_original():
    """report は原典を検査したうえで、原典の有無と行数差を表にする。"""
    for exists, expected_row in ((True, "| あり（1 行） |"), (False, "| なし |")):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_ledger(root, ledger())
            files = {"m/A.scala": "object A\n"} if exists else {"m/Other.scala": "object O\n"}
            pekko, commit = make_source(root, files)
            env = {"PATH": "/usr/bin:/bin:/usr/local/bin", "PEKKO_BASELINE_FILE": str(write_baseline(root, commit))}
            proc = tool(root, "report", "--ledger", str(root / "nodes.yaml"),
                        "--pekko", str(pekko), "--historical", PAST, env=env)
            assert proc.returncode == 0, (proc.stdout, proc.stderr)
            row = [line for line in proc.stdout.splitlines() if line.startswith("| `m/A.scala`")]
            assert row and expected_row in row[0], (expected_row, proc.stdout)
            assert f"| {PAST} |" in row[0], row[0]
            assert "| `m/B.scala` |" not in proc.stdout, proc.stdout
            assert f"原典にファイルが無い {0 if exists else 1} 件" in proc.stderr, proc.stderr
    print("11. report は原典の有無と行数差を出す（あるとき／無いとき）")


def test_report_includes_already_verified_nodes():
    """report は既定で判定済みの全件を出し、--unverified-only で未照合だけに絞る。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_ledger(root, ledger(tag=CURRENT))
        pekko, commit = make_source(root, {"m/A.scala": "object A\n"})
        env = {"PATH": "/usr/bin:/bin:/usr/local/bin", "PEKKO_BASELINE_FILE": str(write_baseline(root, commit))}
        everything = tool(root, "report", "--ledger", str(root / "nodes.yaml"),
                          "--pekko", str(pekko), "--historical", PAST, env=env)
        assert everything.returncode == 0, (everything.stdout, everything.stderr)
        assert "| `m/A.scala` | do_not_port |" in everything.stdout, everything.stdout
        assert "対象 1 件、原典にファイルが無い 0 件" in everything.stderr, everything.stderr
        only_unverified = tool(root, "report", "--ledger", str(root / "nodes.yaml"),
                               "--pekko", str(pekko), "--historical", PAST, "--unverified-only", env=env)
        assert only_unverified.returncode == 0, (only_unverified.stdout, only_unverified.stderr)
        assert "| `m/A.scala` |" not in only_unverified.stdout, only_unverified.stdout
        assert "対象 0 件" in only_unverified.stderr, only_unverified.stderr
        print("12. report は照合済みも出し、--unverified-only で絞る")


def test_report_refuses_an_unprepared_source():
    """原典が無ければ、ファイルの有無を数えずに失敗する（全件「なし」と誤報しない）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_ledger(root, ledger())
        proc = tool(root, "report", "--ledger", str(root / "nodes.yaml"),
                    "--pekko", str(root / "pekko"), "--historical", PAST)
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert proc.stdout == "", proc.stdout
        assert "pekko" in proc.stderr, proc.stderr
        print("13. 原典が無ければ report は失敗する")


def test_apply_and_plan_work_without_pyyaml():
    """PyYAML を解決できない起動（python3 -S）でも、apply と plan が動く。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        path = write_ledger(root, ledger())
        write_ids(root, ["m/A.scala"])
        code = (
            "import sys; sys.path.insert(0, %r);\n"
            "import reverify_against;\n"
            "assert 'yaml' not in sys.modules, 'yaml imported eagerly';\n"
            "sys.exit(reverify_against.main())\n" % str(TOOLS)
        )
        proc = subprocess.run(
            [sys.executable, "-S", "-c", code, "apply",
             "--ledger", str(path), "--ids", str(root / "ids.txt")],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更した: 1 件" in proc.stdout, proc.stdout
        assert f"verified_against: {CURRENT}" in path.read_text(encoding="utf-8")
        print("14. PyYAML なしでも apply が動く")


def main() -> int:
    tests = [
        test_apply_changes_only_the_value,
        test_apply_adds_the_field_after_goal,
        test_apply_refuses_a_status_that_is_not_judged,
        test_apply_refuses_an_id_that_is_not_in_nodes,
        test_apply_refuses_a_tag_that_is_not_current,
        test_apply_keeps_retired_first_and_comments,
        test_apply_is_idempotent_and_reports_no_change,
        test_apply_refuses_duplicate_ids_and_writes_nothing,
        test_dry_run_writes_nothing,
        test_plan_lists_judged_nodes_with_another_tag,
        test_report_table_marks_missing_original,
        test_report_includes_already_verified_nodes,
        test_report_refuses_an_unprepared_source,
        test_apply_and_plan_work_without_pyyaml,
    ]
    assert reverify_against.FIELD == "verified_against"
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except Exception:  # noqa: BLE001
            failures += 1
            print(f"FAIL {test.__name__}:")
            import traceback

            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
