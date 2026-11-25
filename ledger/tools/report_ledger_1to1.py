"""項目 258 (d): 台帳の v2.0.0 の行と、報告書の表の行が 1 対 1 かを機械で確かめる。

対象の報告書は REPORTS に明示する（1 報告書 1 行。増やすときは 1 行足す）。報告書の表は、
見出し行に決まった列がある表だけを読む（ほかの表とコードブロックの中は読まない）。表の行が
機械で読めない報告書があれば、飛ばさずに止まる（終了コード 1）。

check は 5 つの規則を 1 つの CI ステップで全部見る（文言は RULES。要点だけ書く）:
  1. 全 id が台帳にある。`verified_against` の列がある表は、そのセルの値（空欄は null）が
     台帳の値と一致する（列が無い報告書は存在だけ。あとで台帳を正当に照合しても壊れない）
  2. 同じ id が 2 つ以上の報告書に載っていない（1 つの報告書の中でも 2 行にしない）
  3. 宣言する件数（見出しの「175 ノード」など）と、その表の実データ行数が一致する
  4. 報告書にあるのに台帳に無い id が 0 件
  5. 一覧 `docs/baseline/v2.0.0-undocumented-nodes.tsv` が「v2.0.0 で照合済みで、どの報告書にも
     載らない id」と集合として一致する（報告書が入った id は消す運用。残りも欠けも失敗）

list-undocumented は一覧を作り直す（冪等）。category は、そのノードの `verified_against: v2.0.0`
を付けた commit が原典を切り替えた commit 以前なら `before-baseline`、以降なら（その照合を書いた
報告書が無いので）`no-report`。git の履歴から決められない id は `unclassified`。

使い方: `uv run --with pyyaml python3 ledger/tools/report_ledger_1to1.py check` と
`... list-undocumented`（既定はこのリポジトリの台帳・報告書・一覧）。"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import yaml

REPO = Path(__file__).resolve().parents[2]
DEFAULT_LEDGER = REPO / "ledger" / "nodes.yaml"
DEFAULT_UNDOCUMENTED = REPO / "docs" / "baseline" / "v2.0.0-undocumented-nodes.tsv"

# 報告書と一覧が扱う原典のタグ。この 3 報告書と 1 つの一覧は、この基準の記録である。
TAG = "v2.0.0"
TAG_FIELD = "verified_against"
BASELINE_FILE = "tools/pekko-source/pekko-baseline.env"
CATEGORIES = ("before-baseline", "no-report", "other", "unclassified")
HEADER = ("id", "category")

RULES = {
    1: "報告書の全 id が台帳にあり、表の照合タグが台帳の値と一致する",
    2: "同じ id が 2 つ以上の報告書に載っていない",
    3: "報告書の宣言件数と、その表の実データ行数が一致する",
    4: "報告書にあって台帳に無い id が 0 件",
    5: "未記載の一覧と、台帳と報告書から決まる未記載の集合が一致する",
}


class ReportUnreadable(Exception):
    """報告書の表が機械で読めない。飛ばさずに止まる。"""


class ListUnreadable(Exception):
    """未記載の一覧（TSV）が機械で読めない。"""


@dataclass(frozen=True)
class Report:
    """報告書 1 件の読み方。`header_columns` がそろった表だけを報告書の表として読む。"""

    path: str
    header_columns: tuple[str, ...]  # 見出しに、この接頭辞で始まる列が「すべて」あること
    id_column: str  # id が入っている列の見出し接頭辞
    declared: str  # 宣言件数を取り出す正規表現（group 1 が件数）


REPORTS = (
    Report("docs/baseline/v2.0.0-reverification.md", ("id", TAG_FIELD), "id",
           r"^#\s.*?判定済み\s+([\d,]+)\s+ノードの\s+v[\d.]+\s+再照合"),
    Report("docs/baseline/v2.0.0-missing-unported.md", ("id",), "id",
           r"^#\s.*?unported\s+([\d,]+)\s+ノード"),
    Report("docs/baseline/v2.0.0-new-docs-tests.md", (f"パス（{TAG}）",), f"パス（{TAG}）",
           r"^#\s.*?docs のテスト\s+([\d,]+)\s+件"),
)


@dataclass(frozen=True)
class Row:
    """報告書の表の 1 行。`tag` は表に照合タグの列が無ければ None。"""

    report: str
    line: int
    id: str
    tag: str | None = None


@dataclass(frozen=True)
class Violation:
    rule: int
    message: str


def load_ledger(path: Path) -> dict[str, dict]:
    """台帳を id -> ノードの辞書にする。"""
    data = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
    nodes = data.get("nodes") if isinstance(data, dict) else None
    if not isinstance(nodes, list):
        raise ReportUnreadable(f"{path}: nodes の並びが無い")
    for node in nodes:
        if not isinstance(node, dict) or not isinstance(node.get("id"), str):
            raise ReportUnreadable(f"{path}: id の無いノードがある")
    return {node["id"]: node for node in nodes}


def _cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def _is_separator(line: str) -> bool:
    body = line.strip().strip("|")
    return bool(body) and "-" in body and all(ch in " -:|" for ch in body)


def _read_text(root: Path, name: str) -> str:
    try:
        return (Path(root) / name).read_text(encoding="utf-8")
    except OSError as exc:
        raise ReportUnreadable(f"{name}: 読めない（{exc}）") from exc


def _tables(text: str, report: str) -> list[tuple[int, list[str], list[tuple[int, list[str]]]]]:
    """Markdown の表を（見出しの行番号, 見出し, [(行番号, セル)]）の並びにする。

    コードブロックの中の行（シェルの `| grep` など）は表として読まない。"""
    lines = text.splitlines()
    tables = []
    index = 0
    fenced = False
    while index < len(lines):
        if lines[index].lstrip().startswith("```"):
            fenced = not fenced
            index += 1
            continue
        if fenced or not lines[index].lstrip().startswith("|"):
            index += 1
            continue
        start = index
        while index < len(lines) and lines[index].lstrip().startswith("|"):
            index += 1
        block = lines[start:index]
        if len(block) < 2 or not _is_separator(block[1]):
            raise ReportUnreadable(f"{report}:{start + 1}: 見出しと区切りの行が無い")
        rows = [(start + 2 + offset, _cells(line)) for offset, line in enumerate(block[2:])]
        tables.append((start + 1, _cells(block[0]), rows))
    return tables


def _normalize_id(cell: str, report: str, line: int) -> str:
    value = cell.strip().strip("`").strip()
    if not value or any(ch.isspace() for ch in value) or "/" not in value:
        raise ReportUnreadable(f"{report}:{line}: id のセルが読めない: {cell!r}")
    return value


def _normalize_tag(cell: str) -> str | None:
    value = cell.strip().strip("`").strip()
    return None if value in ("", "-", "—") else value


def parse_report(report: Report, root: Path) -> list[Row]:
    """報告書 1 件の表を読む。読めなければ ReportUnreadable。"""
    text = _read_text(root, report.path)
    rows: list[Row] = []
    matched = 0
    for header_line, header, data in _tables(text, report.path):
        if not all(any(cell.startswith(prefix) for cell in header) for prefix in report.header_columns):
            continue
        matched += 1
        id_columns = [k for k, cell in enumerate(header) if cell.startswith(report.id_column)]
        if len(id_columns) != 1:
            raise ReportUnreadable(f"{report.path}:{header_line}: id の列が 1 つに決まらない")
        tag_column = None
        if TAG_FIELD in report.header_columns:
            tag_columns = [k for k, cell in enumerate(header) if cell.startswith(TAG_FIELD)]
            if len(tag_columns) != 1:
                raise ReportUnreadable(f"{report.path}:{header_line}: {TAG_FIELD} の列が 1 つに決まらない")
            tag_column = tag_columns[0]
        for line, cells in data:
            if len(cells) != len(header):
                raise ReportUnreadable(
                    f"{report.path}:{line}: セルが {len(cells)} 個で、見出しの {len(header)} 個と違う")
            rows.append(Row(
                report=report.path,
                line=line,
                id=_normalize_id(cells[id_columns[0]], report.path, line),
                tag=_normalize_tag(cells[tag_column]) if tag_column is not None else None,
            ))
    if matched == 0:
        raise ReportUnreadable(
            f"{report.path}: 報告書の表が無い（見出しに {list(report.header_columns)} の列が要る）")
    return rows


def declared_count(report: Report, root: Path) -> int:
    """報告書が本文で宣言する件数。1 つに決まらなければ ReportUnreadable。"""
    text = _read_text(root, report.path)
    hits = [m.group(1) for m in re.finditer(report.declared, text, re.MULTILINE)]
    if len(hits) != 1:
        raise ReportUnreadable(f"{report.path}: 宣言件数の行が 1 つに決まらない（{len(hits)} 行）")
    return int(hits[0].replace(",", ""))


def collect_rows(root: Path) -> dict[str, list[Row]]:
    """報告書ごとの表の行。読めない報告書があれば ReportUnreadable（飛ばさない）。"""
    return {report.path: parse_report(report, root) for report in REPORTS}


def read_undocumented(path: Path) -> dict[str, str]:
    """未記載の一覧を id -> category の辞書にする。"""
    try:
        text = Path(path).read_text(encoding="utf-8")
    except OSError as exc:
        raise ListUnreadable(f"{path}: 読めない（{exc}）") from exc
    lines = [line for line in text.splitlines() if line.strip()]
    if not lines:
        raise ListUnreadable(f"{path}: 空である")
    header = tuple(cell.strip() for cell in lines[0].split("\t"))
    if header != HEADER:
        raise ListUnreadable(f"{path}: 見出しが {list(HEADER)} でない（{list(header)}）")
    listed: dict[str, str] = {}
    for number, line in enumerate(lines[1:], 2):
        cells = line.split("\t")
        if len(cells) != len(HEADER):
            raise ListUnreadable(f"{path}:{number}: 列が {len(cells)} 個である")
        node_id, category = cells[0].strip(), cells[1].strip()
        if not node_id or node_id in listed:
            raise ListUnreadable(f"{path}:{number}: id が空か重複している（{node_id!r}）")
        if category not in CATEGORIES:
            raise ListUnreadable(
                f"{path}:{number}: category が {list(CATEGORIES)} のどれでもない（{category!r}）")
        listed[node_id] = category
    return listed


def undocumented_set(ledger: dict[str, dict], rows: dict[str, list[Row]]) -> set[str]:
    """台帳で v2.0.0 と照合済みで、どの報告書にも載らない id。"""
    covered = {row.id for report_rows in rows.values() for row in report_rows}
    tagged = {node_id for node_id, node in ledger.items() if node.get(TAG_FIELD) == TAG}
    return tagged - covered


def violations(root: Path, ledger_path: Path, undocumented_path: Path) -> tuple[list[Violation], dict]:
    """5 つの規則の違反を集める。読めない入力は例外（飛ばさない）。"""
    ledger = load_ledger(Path(ledger_path))
    rows = collect_rows(Path(root))
    found: list[Violation] = []

    # 規則 3: 宣言件数とデータ行数。
    for report in REPORTS:
        declared = declared_count(report, Path(root))
        if declared != len(rows[report.path]):
            found.append(Violation(
                3, f"{report.path}: 宣言 {declared} 件 / 表の実データ {len(rows[report.path])} 行"))

    # 規則 1（照合タグ）と規則 4（存在）。存在は全行に効く。
    places: dict[str, list[str]] = {}
    for report in REPORTS:
        for row in rows[report.path]:
            place = f"{row.report}:{row.line}"
            places.setdefault(row.id, []).append(place)
            node = ledger.get(row.id)
            if node is None:
                found.append(Violation(4, f"{place}: 台帳に無い id {row.id}"))
            elif row.tag is not None and row.tag != node.get(TAG_FIELD):
                found.append(Violation(
                    1, f"{place}: 表は {row.tag} だが台帳は {node.get(TAG_FIELD)}（{row.id}）"))

    # 規則 2: 同じ id は 1 行だけ。
    for node_id, where in sorted(places.items()):
        if len(where) > 1:
            reports = len({place.split(":", 1)[0] for place in where})
            found.append(Violation(
                2, f"{node_id}: {reports} 報告書 {len(where)} 行に載っている（{', '.join(where)}）"))

    # 規則 5: 一覧との集合の一致（追加も削除も）。
    listed = read_undocumented(Path(undocumented_path))
    expected = undocumented_set(ledger, rows)
    for node_id in sorted(set(listed) - expected):
        found.append(Violation(
            5, f"{node_id}: 一覧にあるが未記載でない（報告書に載った、または照合タグが {TAG} でない）"))
    for node_id in sorted(expected - set(listed)):
        found.append(Violation(5, f"{node_id}: 未記載なのに一覧に無い（一覧を作り直す）"))

    summary = {"reports": len(REPORTS), "rows": sum(len(rows[r.path]) for r in REPORTS),
               "tagged": sum(1 for n in ledger.values() if n.get(TAG_FIELD) == TAG),
               "undocumented": len(expected), "listed": len(listed)}
    return found, summary


def cmd_check(args) -> int:
    try:
        found, summary = violations(args.root, args.ledger, args.undocumented)
    except (ReportUnreadable, ListUnreadable) as exc:
        print(f"機械で読めない入力がある（飛ばさずに止まる）: {exc}", file=sys.stderr)
        return 1
    print(f"報告書 {summary['reports']} 件 / 表の行 {summary['rows']} 行、"
          f"台帳の {TAG} {summary['tagged']} 件、一覧 {summary['listed']} 件"
          f"（未記載 {summary['undocumented']} 件）")
    for rule in sorted(RULES):
        items = [violation.message for violation in found if violation.rule == rule]
        print(f"検査{rule} {RULES[rule]}: {'OK' if not items else f'違反 {len(items)} 件'}")
        for message in items:
            print(f"  - {message}")
    if found:
        print(f"1 対 1 でない（検査 {sorted({v.rule for v in found})}）")
    return 1 if found else 0


def tag_lines(text: str) -> dict[str, int]:
    """台帳のテキストから、ノードごとの `verified_against` 行の行番号を出す。"""
    lines: dict[str, int] = {}
    current = None
    for number, line in enumerate(text.splitlines(), 1):
        match = re.match(r"^- id: (.+)$", line)
        if match:
            current = match.group(1).strip()
        elif current is not None and re.match(r"^  verified_against:", line):
            lines[current] = number
    return lines


class Classifier:
    """`verified_against: v2.0.0` を付けた commit から category を決める。"""

    def __init__(self, root: Path, ledger_path: Path, out=None):
        self.root = Path(root)
        self.ledger_path = Path(ledger_path)
        self.out = out if out is not None else sys.stderr
        self._switch: str | None = None
        self._switch_done = False
        self._warned = False

    def _warn(self, message: str) -> None:
        if not self._warned:
            print(f"警告: {message}（決められない id は unclassified にする）", file=self.out)
            self._warned = True

    def _git(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git", "-C", str(self.root), *args], capture_output=True, text=True, check=False)

    def switch_commit(self) -> str | None:
        if self._switch_done:
            return self._switch
        self._switch_done = True
        proc = self._git("log", "--format=%H", "--reverse", "-S",
                         f"PEKKO_BASELINE_TAG={TAG}", "--", BASELINE_FILE)
        commits = proc.stdout.split()
        self._switch = commits[0] if proc.returncode == 0 and commits else None
        if self._switch is None:
            self._warn(f"{BASELINE_FILE} で {TAG} に切り替えた commit を git の履歴から決められない")
        return self._switch

    def introduced(self, line: int) -> str | None:
        try:
            relative = self.ledger_path.resolve().relative_to(self.root.resolve())
        except ValueError:
            relative = self.ledger_path
        proc = self._git("blame", "--line-porcelain", "-L", f"{line},{line}", "--", str(relative))
        first = proc.stdout.split("\n", 1)[0].split()
        if proc.returncode != 0 or not first:
            self._warn(f"{self.ledger_path}:{line} を git blame できない")
            return None
        return first[0]

    def is_ancestor(self, commit: str, of: str) -> bool | None:
        proc = self._git("merge-base", "--is-ancestor", commit, of)
        return True if proc.returncode == 0 else (False if proc.returncode == 1 else None)

    def __call__(self, node_id: str, line: int | None) -> str:
        switch = self.switch_commit()
        if line is None or switch is None:
            return "unclassified"
        commit = self.introduced(line)
        before = None if commit is None else self.is_ancestor(commit, switch)
        if before is None:
            return "unclassified"
        return "before-baseline" if before else "no-report"


def list_undocumented(root: Path, ledger_path: Path, undocumented_path: Path,
                      classify: Callable[[str], str] | None = None, out=None) -> dict[str, str]:
    """未記載の一覧を id と category で作り直す（冪等）。"""
    out = out if out is not None else sys.stderr
    ledger = load_ledger(Path(ledger_path))
    expected = sorted(undocumented_set(ledger, collect_rows(Path(root))))
    if classify is None:
        line_of = tag_lines(Path(ledger_path).read_text(encoding="utf-8"))
        classifier = Classifier(root, ledger_path, out)
        classify = lambda node_id: classifier(node_id, line_of.get(node_id))  # noqa: E731
    categories = {node_id: classify(node_id) for node_id in expected}
    bad = sorted({category for category in categories.values() if category not in CATEGORIES})
    if bad:
        raise ListUnreadable(f"category が {list(CATEGORIES)} のどれでもない: {bad}")
    lines = ["\t".join(HEADER)] + [f"{node_id}\t{categories[node_id]}" for node_id in expected]
    path = Path(undocumented_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    counts = {category: list(categories.values()).count(category) for category in CATEGORIES}
    detail = "、".join(f"{category} {counts[category]}" for category in CATEGORIES)
    print(f"{path}: {len(expected)} 件（{detail}）", file=out)
    return categories


def cmd_list(args) -> int:
    try:
        list_undocumented(args.root, args.ledger, args.undocumented)
    except (ReportUnreadable, ListUnreadable) as exc:
        print(f"一覧を作れない: {exc}", file=sys.stderr)
        return 1
    return 0



def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name, func, help_text in (
        ("check", cmd_check, "報告書・台帳・未記載の一覧が 1 対 1 か検査する（CI）"),
        ("list-undocumented", cmd_list, "未記載の一覧を作り直す（冪等）"),
    ):
        command = sub.add_parser(name, help=help_text)
        command.add_argument("--root", default=str(REPO))
        command.add_argument("--ledger", default=str(DEFAULT_LEDGER))
        command.add_argument("--undocumented", default=str(DEFAULT_UNDOCUMENTED))
        command.set_defaults(func=func)
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
