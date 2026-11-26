"""#206 の人の決定（M2）：機械の判定とは別のファイルに置き、TSV を作り直しても消えない。"""
from pathlib import Path

from origin_evidence import STRONG

DECISIONS_DEFAULT = Path("docs") / "design" / "issue-206-origin-decisions.tsv"

DECISION_FLAGGED = "@要確認"

SPDX_APACHE = "Apache-2.0"

SPDX_DUAL = "MIT OR Apache-2.0"

HELD = "保留"

DECISION_HEADER = ["対象", "決定", "決めた人", "日付", "理由"]


def load_decisions(path):
    """人の決定のファイルを読む。機械の判定とは別に置き、TSV を作り直しても消えない。

    `対象` は、リポジトリのルートからの相対パスか、`@要確認`（要確認の行すべて）。
    `決定` は付ける SPDX の式。`Apache-2.0`、`MIT OR Apache-2.0`、または原典の第三者の
    ライセンスを足した式（M3。例 `Apache-2.0 AND Unlicense`）。
    """
    if not path.exists():
        return []
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if number == 1:
            if line.split("\t") != DECISION_HEADER:
                raise SystemExit(f"{path}: 見出しは {DECISION_HEADER} でなければならない")
            continue
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != len(DECISION_HEADER) or not all(cell.strip() for cell in cells):
            raise SystemExit(f"{path}:{number}: 5 列すべてを埋める")
        rows.append((number, *cells))
    return rows

def apply_decisions(rows, decisions, decisions_name):
    """機械の判定に人の決定を重ね、付ける SPDX と決定の出典を返す（path ごと）。

    要確認の一括の決定を先に、ファイルごとの決定を後に重ねる（ファイルごとが勝つ）。
    `MIT OR Apache-2.0` は、ファイルごとの決定でしか付かない（元帥の決定 1）。
    """
    by_path = {row[0]: row for row in rows}
    batch = {}
    single = {}
    for number, target, spdx, who, date, reason in decisions:
        source = f"{decisions_name}:{number}（{who} {date}）"
        if target == DECISION_FLAGGED:
            if spdx != SPDX_APACHE:
                raise SystemExit(f"{source}: 要確認の一括の決定は {SPDX_APACHE} だけ")
            if batch:
                raise SystemExit(f"{source}: 要確認の一括の決定が 2 つある")
            batch = {"spdx": spdx, "source": source}
            continue
        if target.startswith("@"):
            raise SystemExit(f"{source}: 未知の対象 {target}")
        if target not in by_path:
            raise SystemExit(f"{source}: 一覧に無いファイル {target}")
        if target in single:
            raise SystemExit(f"{source}: {target} の決定が 2 つある")
        kind = by_path[target][3]
        if spdx == SPDX_DUAL and kind in STRONG:
            raise SystemExit(f"{source}: {target} は強い根拠（{kind}）があるので {SPDX_DUAL} にできない")
        single[target] = {"spdx": spdx, "source": source}

    result = {}
    for rel, scope, verdict, kind, snippet, flag, reason in rows:
        decision = single.get(rel) or (batch if flag else None)
        if decision:
            result[rel] = (decision["spdx"], decision["source"])
        elif verdict == "Pekko 由来" and not flag:
            result[rel] = (SPDX_APACHE, "判定")
        else:
            result[rel] = (HELD, "")
    return result
