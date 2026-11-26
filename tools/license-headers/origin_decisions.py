"""#206 の人の決定（M2）：機械の判定とは別のファイルに置き、TSV を作り直しても消えない。"""
import datetime
import re
from pathlib import Path

from origin_evidence import STRONG

DECISIONS_DEFAULT = Path("docs") / "design" / "issue-206-origin-decisions.tsv"

DECISION_FLAGGED = "@要確認"

SPDX_APACHE = "Apache-2.0"

SPDX_DUAL = "MIT OR Apache-2.0"

HELD = "保留"

DECISION_HEADER = ["対象", "決定", "決めた人", "日付", "理由"]

# 決定の式の文法（設計レビュー 3 回目の M2）。許すのは `Apache-2.0`、`MIT OR Apache-2.0`、
# `Apache-2.0 AND <id>` の 3 つだけで、<id> はこの固定の一覧から選ぶ。一覧を広げるのは設計の変更。
EXTERNAL_IDS = ("BSD-2-Clause", "BSD-3-Clause", "CC0-1.0", "Unlicense")

DATE = re.compile(r"\d{4}-\d{2}-\d{2}")

MIT_TOKEN = re.compile(r"\bmit\b", re.IGNORECASE)


def valid_expression(spdx):
    """決定の式が 3 つの形のどれかと、綴りまで一致するか。"""
    return spdx in (SPDX_APACHE, SPDX_DUAL) or spdx in {f"{SPDX_APACHE} AND {i}" for i in EXTERNAL_IDS}


def mentions_mit(spdx):
    """`MIT` を含む式は、綴りや並びによらず、すべて MIT として扱う（M2）。"""
    return bool(MIT_TOKEN.search(spdx or ""))


def valid_date(text):
    if not DATE.fullmatch(text):
        return False
    try:
        datetime.date.fromisoformat(text)
    except ValueError:
        return False
    return True


def load_decisions(path):
    """人の決定のファイルを読む。機械の判定とは別に置き、TSV を作り直しても消えない。

    `対象` は、リポジトリのルートからの相対パスか、`@要確認`（要確認の行すべて）。
    `決定` は付ける SPDX の式で、`valid_expression` の 3 つの形に限る。`日付` は YYYY-MM-DD。
    `#` で始まる行は提案で、読まない。提案の行（`#` を外すと 5 列になる行）は `決めた人` を空に
    しておく。`#` を外しただけでは決定にならないようにするため（設計レビュー 3 回目の L1 (c)）。
    """
    if not path.exists():
        return []
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if number == 1:
            if line.split("\t") != DECISION_HEADER:
                raise SystemExit(f"{path}: 見出しは {DECISION_HEADER} でなければならない")
            continue
        if not line.strip():
            continue
        if line.startswith("#"):
            proposal = line.lstrip("#").lstrip(" ").split("\t")
            if len(proposal) == len(DECISION_HEADER) and proposal[2].strip():
                raise SystemExit(f"{path}:{number}: 提案の行は `決めた人` を空にする（`#` を外すだけで決定にならないように）")
            continue
        cells = line.split("\t")
        if len(cells) != len(DECISION_HEADER) or not all(cell.strip() for cell in cells):
            raise SystemExit(f"{path}:{number}: 5 列すべてを埋める")
        if not valid_expression(cells[1]):
            raise SystemExit(
                f"{path}:{number}: 決定の式 {cells[1]!r} は使えない。"
                f"{SPDX_APACHE}、{SPDX_DUAL}、{SPDX_APACHE} AND <{'|'.join(EXTERNAL_IDS)}> のどれか"
            )
        if not valid_date(cells[3]):
            raise SystemExit(f"{path}:{number}: 日付 {cells[3]!r} は YYYY-MM-DD の実在する日付にする")
        rows.append((number, *cells))
    return rows

def apply_decisions(rows, decisions, decisions_name):
    """機械の判定に人の決定を重ね、付ける SPDX と決定の出典を返す（path ごと）。

    要確認の一括の決定を先に、ファイルごとの決定を後に重ねる（ファイルごとが勝つ）。
    `MIT` を含む式は、ファイルごとの決定でしか付かない（元帥の決定 1）。
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
        if mentions_mit(spdx) and kind in STRONG:
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


MANIFEST_COLUMNS = ("path", "走査範囲", "判定", "根拠の種類", "根拠の中身", "要確認", "要確認の理由")


def read_manifest(path):
    """線引き一覧の TSV を (行番号, 列名→値) の並びで読む。"""
    lines = path.read_text(encoding="utf-8").splitlines()
    header = lines[0].split("\t")
    return [(number, dict(zip(header, line.split("\t")))) for number, line in enumerate(lines[1:], start=2)]


def manifest_errors(manifest, decisions, decisions_name, spdx_by_path):
    """`--check --manifest` のうち、原典の要らない部分（設計レビュー 3 回目の H1、M1、M2）。

    manifest は read_manifest の戻り値、decisions は load_decisions の戻り値。spdx_by_path は、
    追跡している検査対象のファイルのパスから、ファイル先頭の SPDX の式（無ければ None）への辞書。
    SPDX が無いこと（C1）とヘッダーの形（C2〜C4、C6）はヘッダーツールが見る。

    - TSV の `付けるSPDX` と `決定の出典` を、`判定`、`根拠の種類`、`要確認` の列と決定の
      ファイルから計算し直し、食い違えば落とす（TSV の手直し）。
    - `MIT` を含む式は、決定のファイルにそのパスの行があるときだけ許す（I9 をファイルにも効かせる）。
    - TSV に無い新しいファイルは `Apache-2.0` だけ（決定の行は TSV にあるパスしか取れない）。
    - 式は決定の文法の 3 つの形に限る。
    """
    rows = [tuple(cells.get(column, "") for column in MANIFEST_COLUMNS) for _number, cells in manifest]
    decided = apply_decisions(rows, decisions, decisions_name)
    mit_decided = {target for _n, target, spdx, *_rest in decisions if not target.startswith("@") and mentions_mit(spdx)}
    errors = []
    for number, cells in manifest:
        rel = cells.get("path", "")
        expected = decided[rel]
        actual = (cells.get("付けるSPDX", ""), cells.get("決定の出典", ""))
        if actual != expected:
            errors.append(f"manifest-tampered {rel} line={number} expected={expected} actual={actual}")
        if rel not in spdx_by_path:
            errors.append(f"manifest-missing-file {rel} line={number}")
    for rel, spdx in sorted(spdx_by_path.items()):
        if spdx is None:
            continue
        if not valid_expression(spdx):
            errors.append(f"spdx-grammar {rel} actual={spdx!r}")
        if mentions_mit(spdx) and rel not in mit_decided:
            errors.append(f"mit-without-decision {rel} actual={spdx!r}")
        if rel in decided:
            expected = decided[rel][0]
            if expected == HELD:
                errors.append(f"held-has-spdx {rel} actual={spdx!r}")
            elif spdx != expected:
                errors.append(f"manifest-mismatch {rel} expected={expected!r} actual={spdx!r}")
        elif spdx != SPDX_APACHE:
            errors.append(f"new-file-not-apache {rel} actual={spdx!r}")
    return errors
