#!/usr/bin/env python3
"""原典 v2.0.0 の移動と削除を台帳へ適用する道具（ledger/tools/relocate_nodes.py）の回帰テスト。

確かめること:
- 移動は `- id:` を付け替え、クラス名が変わる移動だけ `name` と `pekko.class` を直し、
  `status`・`cases`・`goal`・件数はそのまま残す
- 削除は `status: do_not_port`、`triage.kind: do_not_port`、根拠表の理由、現行タグにする
- 旧 id の文字列参照（ゴール文書など）を置き換える。根拠表と凍結した過去の基準は残す
- 対象ノードの外の行（行頭の注記、ほかのノード）は変えない
- 適用済みなら冪等（2 回目は変更 0 件で成功する）
- 根拠表と台帳が食い違うとき・旧 id の参照が残るときは check が 1 で落ちる
  （わざと壊した入力で確かめる）
- PyYAML なし（`python3 -S`）でも apply が動く

使い方:
    uv run --with pyyaml python3 ledger/tools/relocate_nodes_test.py
"""
import subprocess
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import pekko_baseline  # noqa: E402
import relocate_table  # noqa: E402

TAG = pekko_baseline.load().tag

SHA = "0123456789abcdef0123456789abcdef01234567"
DATE = "2026-11-25"

NODE = """- id: {id}
  name: {name}
  kind: test
  pekko:
    module: {module}
    class: {klass}
    test_cases_approx: {approx}
  status: {status}
  triage:
    kind: {triage_kind}
    reason: {reason}
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
  goal: {goal}
{tag}
"""

HEAD = "# 台帳の見出し（行頭の注記）\n\nnodes:\n"

def _node(node_id, *, name=None, klass=None, status="unported", triage_kind="pending",
          reason="null", approx=1, goal="null", tag="") -> str:
    module = node_id.split("/", 1)[0]
    klass = klass or Path(node_id).stem
    name = name or f"{module}:{klass}"
    return NODE.format(id=node_id, name=name, module=module, klass=klass, approx=approx,
                       status=status, triage_kind=triage_kind, reason=reason, goal=goal, tag=tag)


def ledger(*, moved: bool = False, deleted: bool = True) -> str:
    body = ""
    old = "m/New.scala" if moved else "m/Old.scala"
    body += _node(old, name="m:New" if moved else "m:Old")
    body += "- id: m/Kept.scala\n  name: m:Kept\n  kind: test\n  pekko:\n    module: m\n" \
            "    class: Kept\n    test_cases_approx: 2\n  status: unported\n  triage:\n" \
            "    kind: pending\n    reason: null\n  needs:\n    testkit: []\n    premise_flags: []\n" \
            "  depends_on_impl:\n    modules: []\n    coverage: approx\n  cases:\n    total: null\n" \
            "    ported: null\n    passing: null\n    excluded: []\n  rust:\n    crate: null\n" \
            "    test_file: null\n  goal: null\n"
    body += _node("m/Deleted.scala", status="do_not_port" if not deleted else "unported")
    return HEAD + body


TABLE_HEADER = "kind\told_id\tnew_id\treason\tdeleted_in\tconfirmed_at\n"


def make_table(root: Path, *, move: bool = True, delete: bool = True) -> Path:
    rows = []
    if move:
        rows.append("\t".join(["move", "m/Old.scala", "m/New.scala", "クラス名が変わる改名",
                               SHA, DATE]))
    if delete:
        rows.append("\t".join(["delete", "m/Deleted.scala", "-", "原典から削除", SHA, DATE]))
    path = root / "docs" / "baseline" / "port-targets.tsv"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(TABLE_HEADER + "\n".join(rows) + "\n", encoding="utf-8")
    return path


def make_root(base: Path, *, moved: bool = False) -> Path:
    root = base
    (root / "ledger").mkdir(parents=True, exist_ok=True)
    (root / "docs" / "goals").mkdir(parents=True, exist_ok=True)
    history = root / relocate_table.history_dirs()[0]
    history.mkdir(parents=True, exist_ok=True)
    (root / "ledger" / "nodes.yaml").write_text(ledger(moved=moved), encoding="utf-8")
    (root / "docs" / "goals" / "g.md").write_text(
        "対象: `m/Old.scala`（旧 id の参照）\n", encoding="utf-8")
    # 凍結した過去の基準は、旧 id を持っていても書き換えない。
    (history / "ledger.yaml").write_text("file: m/Old.scala\n", encoding="utf-8")
    return root


def tool(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOLS / "relocate_nodes.py"),
         "--root", str(root),
         "--table", str(root / "docs" / "baseline" / "port-targets.tsv"),
         "--ledger", str(root / "ledger" / "nodes.yaml"), *args],
        capture_output=True, text=True, check=False,
    )


def test_apply_moves_and_deletes():
    """移動と削除を適用し、status・cases・goal と、ノードの外の行を保つ。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root)
        before = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        proc = tool(root, "apply")
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        assert "- id: m/New.scala" in after and "- id: m/Old.scala" not in after, after
        assert "  name: m:New" in after, after
        assert "    class: New" in after, after
        # ノードの外（見出しと、無関係のノード）は 1 バイトも変わらない。
        assert after.startswith(before[:before.index("- id: m/Old.scala")]), after[:200]
        assert "- id: m/Kept.scala" in after and "class: Kept" in after, after
        # 削除は status・triage・verified_against だけを変える。
        deleted = after[after.index("- id: m/Deleted.scala"):]
        assert "  status: do_not_port" in deleted
        assert "    kind: do_not_port" in deleted
        assert f"    reason: {relocate_table.DELETE_REASON.format(short=SHA[:10], date=DATE)}" in deleted
        assert f"  verified_against: {TAG}" in deleted
        # 移動したノードの status・cases・goal はそのまま。
        moved = after[after.index("- id: m/New.scala"):after.index("- id: m/Kept.scala")]
        assert "  status: unported" in moved and "    total: null" in moved and "  goal: null" in moved
        # ゴール文書の旧 id を新 id に置き換え、凍結した過去の基準は残す。
        assert "`m/New.scala`" in (root / "docs" / "goals" / "g.md").read_text(encoding="utf-8")
        frozen = root / relocate_table.history_dirs()[0] / "ledger.yaml"
        assert "m/Old.scala" in frozen.read_text(encoding="utf-8")
        print("1. 移動と削除を適用し、対象の欄だけを変える")


def test_check_passes_after_apply():
    """適用後は根拠表と台帳が一致し、旧 id の参照は 0 件になる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root)
        assert tool(root, "apply").returncode == 0
        proc = tool(root, "check")
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "旧 id の参照は 0 件" in proc.stdout, proc.stdout
        print("2. 適用後は check が通る")


def test_check_fails_when_the_ledger_still_has_the_old_id():
    """わざと壊す：移動を適用せず旧 id のままにすると、check が 1 で落ちる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        # 削除だけを適用し、そのあとで移動も含む根拠表にして check する。
        make_table(root, move=False, delete=True)
        assert tool(root, "apply").returncode == 0
        make_table(root, move=True, delete=True)
        ledger_path = root / "ledger" / "nodes.yaml"
        original = ledger_path.read_text(encoding="utf-8")
        proc = tool(root, "check")
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "- id: m/Old.scala" in proc.stderr or "m/Old.scala" in proc.stderr, proc.stderr
        assert ledger_path.read_text(encoding="utf-8") == original
        print("3. 旧 id が台帳に残ると check は落ちる")


def test_check_fails_when_a_reference_remains():
    """わざと壊す：旧 id を書いた文書を足すと、check が 1 で落ちる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root)
        assert tool(root, "apply").returncode == 0
        (root / "docs" / "goals" / "broken.md").write_text("参照: m/Old.scala\n", encoding="utf-8")
        proc = tool(root, "check")
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "旧 id の参照が残っている" in proc.stderr, proc.stderr
        print("4. 旧 id の参照が残ると check は落ちる")


def test_apply_refuses_when_the_new_id_already_exists():
    """移動先のノードが既にあるときは、台帳を書き換えずに失敗する。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root, delete=False)
        ledger_path = root / "ledger" / "nodes.yaml"
        text = ledger_path.read_text(encoding="utf-8").replace(
            "nodes:\n", "nodes:\n" + _node("m/New.scala", name="m:New"))
        ledger_path.write_text(text, encoding="utf-8")
        proc = tool(root, "apply")
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "既にある" in proc.stderr, proc.stderr
        assert ledger_path.read_text(encoding="utf-8") == text
        print("5. 移動先が既にあるときは書き換えない")


def test_apply_is_idempotent():
    """2 回目の apply は変更 0 件で成功し、台帳も参照も変えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root)
        assert tool(root, "apply").returncode == 0
        once = (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        proc = tool(root, "apply")
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "move 0 件、delete 0 件" in proc.stdout, proc.stdout
        assert (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8") == once
        print("6. 2 回目は変更 0 件")


def test_read_table_rejects_bad_input():
    """根拠表の書式（kind・40 hex・実在する暦日・重複）を検査する。"""
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "table.tsv"
        good = ["move", "m/Old.scala", "m/New.scala", "理由", SHA, DATE]
        for bad, needle in (
            (["rename", "m/Old.scala", "m/New.scala", "理由", SHA, DATE], "kind"),
            (["move", "m/Old.scala", "m/New.scala", "理由", "abc", DATE], "40 桁"),
            (["move", "m/Old.scala", "m/New.scala", "理由", SHA, "2026-02-30"], "暦日"),
            (["delete", "m/Old.scala", "m/New.scala", "理由", SHA, DATE], "new_id"),
        ):
            path.write_text(TABLE_HEADER + "\t".join(bad) + "\n", encoding="utf-8")
            try:
                relocate_table.read_table(path)
            except ValueError as error:
                assert needle in str(error), (needle, error)
            else:
                raise AssertionError(f"受け付けてはいけない: {bad}")
        rows = "\t".join(good) + "\n" + "\t".join(good) + "\n"
        path.write_text(TABLE_HEADER + rows, encoding="utf-8")
        try:
            relocate_table.read_table(path)
        except ValueError as error:
            assert "重複" in str(error), error
        else:
            raise AssertionError("old_id の重複を受け付けた")
        print("7. 根拠表の書式違反を拒否する")


def test_apply_works_without_pyyaml():
    """PyYAML を解決できない起動（python3 -S）でも apply が動く。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = make_root(Path(temporary))
        make_table(root)
        proc = subprocess.run(
            [sys.executable, "-S", str(TOOLS / "relocate_nodes.py"),
             "--root", str(root),
             "--table", str(root / "docs" / "baseline" / "port-targets.tsv"),
             "--ledger", str(root / "ledger" / "nodes.yaml"), "apply"],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "- id: m/New.scala" in (root / "ledger" / "nodes.yaml").read_text(encoding="utf-8")
        print("8. PyYAML なしでも apply が動く")


def main() -> int:
    tests = [
        test_apply_moves_and_deletes,
        test_check_passes_after_apply,
        test_check_fails_when_the_ledger_still_has_the_old_id,
        test_check_fails_when_a_reference_remains,
        test_apply_refuses_when_the_new_id_already_exists,
        test_apply_is_idempotent,
        test_read_table_rejects_bad_input,
        test_apply_works_without_pyyaml,
    ]
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
