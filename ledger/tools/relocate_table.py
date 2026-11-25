#!/usr/bin/env python3
"""原典 v2.0.0 の移動・削除の根拠表（TSV）と、旧 id の文字列参照を扱う。

`relocate_nodes.py` の入力（根拠表）と、旧 id の参照の検索・置き換えをここに置く。
PyYAML を使わない（`apply` を PyYAML なしで動かす経路を壊さない）。
"""
import datetime
import re
from dataclasses import dataclass
from pathlib import Path

# 旧 id の文字列参照を探して置き換える範囲。根拠表そのものと、凍結した過去の基準は履歴なので除く。
SCOPE = ("ledger", "docs", ".takt", "scripts")
SKIP_DIRS = ("__pycache__", ".git", "node_modules")


def history_dirs() -> tuple[str, ...]:
    """凍結した過去の基準のディレクトリ（履歴なので、旧 id の参照を探さない）。"""
    import pekko_baseline

    return tuple(f"docs/baseline/pekko-{tag}/" for tag in sorted(pekko_baseline.load().historical))

DELETE_REASON = "原典 v2.0.0 で削除（削除コミット {short}・確認日 {date}）"
HEADER = ["kind", "old_id", "new_id", "reason", "deleted_in", "confirmed_at"]


@dataclass(frozen=True)
class Row:
    kind: str
    old_id: str
    new_id: str
    reason: str
    deleted_in: str
    confirmed_at: str

    @property
    def short(self) -> str:
        return self.deleted_in[:10]

    @property
    def delete_reason(self) -> str:
        return DELETE_REASON.format(short=self.short, date=self.confirmed_at)


def _valid_date(text: str) -> bool:
    try:
        return datetime.date.fromisoformat(text).isoformat() == text
    except ValueError:
        return False


def read_table(path: str | Path) -> list[Row]:
    """根拠表（TSV）を読み、1 行 1 件のレコードにする。書式の違反は ValueError。"""
    rows: list[Row] = []
    header_seen = False
    for number, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split("\t")
        if not header_seen:
            if fields != HEADER:
                raise ValueError(f"{path}:{number}: 見出しが {HEADER} でない")
            header_seen = True
            continue
        if len(fields) != len(HEADER):
            raise ValueError(f"{path}:{number}: 欄の数が {len(HEADER)} でない: {line!r}")
        rows.append(Row(*fields))
    if not rows:
        raise ValueError(f"{path}: 行が無い")
    for row in rows:
        if row.kind not in ("move", "delete"):
            raise ValueError(f"{path}: kind が move か delete でない: {row.kind!r}")
        if not row.old_id or not row.reason or not row.confirmed_at:
            raise ValueError(f"{path}: old_id・reason・confirmed_at が空: {row.old_id!r}")
        if not re.fullmatch(r"[0-9a-f]{40}", row.deleted_in):
            raise ValueError(f"{path}: deleted_in が 40 桁の hex でない: {row.deleted_in!r}")
        if not _valid_date(row.confirmed_at):
            raise ValueError(f"{path}: confirmed_at が実在する暦日でない: {row.confirmed_at!r}")
        if row.kind == "move":
            if not row.new_id or row.new_id == "-":
                raise ValueError(f"{path}: move に new_id が無い: {row.old_id!r}")
            if row.old_id == row.new_id:
                raise ValueError(f"{path}: move の old_id と new_id が同じ: {row.old_id!r}")
        elif row.new_id != "-":
            raise ValueError(f"{path}: delete の new_id は - である: {row.new_id!r}")
    old_ids = [r.old_id for r in rows]
    if len(set(old_ids)) != len(old_ids):
        raise ValueError(f"{path}: old_id が重複している")
    new_ids = [r.new_id for r in rows if r.kind == "move"]
    if len(set(new_ids)) != len(new_ids):
        raise ValueError(f"{path}: new_id が重複している")
    return rows


def _scope_files(root: Path, table: Path) -> list[Path]:
    """旧 id の文字列参照を探すファイル（根拠表と凍結した過去の基準を除く）。"""
    files: list[Path] = []
    table = table.resolve()
    history = history_dirs()
    for name in SCOPE:
        base = root / name
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.resolve() == table:
                continue
            relative = path.relative_to(root).as_posix()
            if any(part in SKIP_DIRS for part in path.relative_to(root).parts):
                continue
            if any(relative.startswith(d) for d in history):
                continue
            files.append(path)
    return files


def find_references(root: Path, table: Path, rows: list[Row]) -> list[str]:
    """移動した旧 id の文字列参照を返す（`path:line: 行`）。"""
    moved = [r.old_id for r in rows if r.kind == "move"]
    hits: list[str] = []
    for path in _scope_files(root, table):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for number, line in enumerate(text.splitlines(), 1):
            for old_id in moved:
                if old_id in line:
                    hits.append(f"{path.relative_to(root)}:{number}: {line.strip()}")
    return hits


def replace_references(root: Path, table: Path, rows: list[Row], dry_run: bool,
                       ledger: Path | None = None) -> list[str]:
    """移動した旧 id の文字列参照を新 id に置き換える。置き換えた件数をファイル別に返す。

    台帳は `ledger_lines.py` で行を書き換えるので、この文字列置換の対象にしない。
    """
    moves = [(r.old_id, r.new_id) for r in rows if r.kind == "move"]
    changed: list[str] = []
    for path in _scope_files(root, table):
        if ledger is not None and path.resolve() == ledger.resolve():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        replaced = text
        count = 0
        for old_id, new_id in moves:
            count += replaced.count(old_id)
            replaced = replaced.replace(old_id, new_id)
        if replaced != text:
            changed.append(f"{path.relative_to(root)}: {count} 件")
            if not dry_run:
                path.write_text(replaced, encoding="utf-8")
    return changed
