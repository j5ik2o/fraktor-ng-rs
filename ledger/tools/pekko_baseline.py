#!/usr/bin/env python3
"""移植の原典にする Pekko の基準タグを扱う（基準の読み込み、台帳の照合済みタグの検査）。

基準の正は `tools/pekko-source/pekko-baseline.env`。現行タグ（PEKKO_BASELINE_TAG）と
その commit、過去の基準（PEKKO_HISTORICAL_BASELINES）を持つ。環境変数
PEKKO_BASELINE_FILE で別の基準ファイルを指せる（テスト用）。

使い方:
    # 原典（PEKKO_ORIGINAL_ROOT、無ければ pekko/）が変更のない現行の基準 commit なら、
    # その基準タグを 1 行で出す（違えば終了コード 1）
    python3 ledger/tools/pekko_baseline.py source-tag
    # 同じ検査をして、原典の絶対パスを出す（triage・port-test が読む場所）
    python3 ledger/tools/pekko_baseline.py source-root

    # 台帳の verified_against を検査し、照合済みタグ別の件数を出す（違反があれば終了コード 1）
    uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml
    #   --list-unverified を付けると、現行タグで未照合のノードを列挙する
    #   --pekko pekko を付けると、現行の原典にノードのファイルがあるかも数える

    # 現役の経路（.takt、原典準備、台帳ツール、CI）に過去の基準タグと commit が残っていないか
    python3 ledger/tools/pekko_baseline.py check-refs

台帳の規則（`docs/decisions/ledger-schema-draft.md` の verified_against）:
  判定済みの status（green / do_not_port / not_built）のノードは verified_against に
  基準ファイルが知っているタグを持つ。それ以外の status のノードは欄を持たないか null。
  「現行タグで未照合」は、判定済みで verified_against が現行タグと違うノードを指す。

PyYAML は check-ledger のときだけ遅延 import する（record_already_green.py は
`--with pyyaml` なしで本モジュールを import する）。
"""
import argparse
import collections
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_BASELINE_FILE = REPO / "tools" / "pekko-source" / "pekko-baseline.env"

# 判定済み（原典と照合して決めた）status。照合したタグを必ず持つ。
JUDGED_STATUSES = ("green", "do_not_port", "not_built")

FIELD = "verified_against"

# 現役の経路。ここに過去の基準タグや commit が書かれていたら、旧原典を使う経路が残っている。
ACTIVE_PATHS = (
    ".takt",
    ".github/workflows",
    "tools/pekko-source",
    "ledger/tools",
    "ledger/README.md",
    "ledger/capabilities.yaml",
    ".gitignore",
)
# 現役の経路の中にあるが、凍結した過去の基準を読むための道具。過去のタグを書いてよい。
HISTORY_TOOLS = (
    # 基準ファイル自身。過去の基準を履歴として列挙する
    "tools/pekko-source/pekko-baseline.env",
    # docs/baseline/pekko-v1.7.1/ledger.yaml から台帳を種まきした変換器
    "ledger/tools/convert_baseline.py",
    # docs/baseline/pekko-v1.7.1/supplement-recount.md を生成した再計測
    "ledger/tools/recount_supplement.py",
)


class BaselineError(Exception):
    """基準ファイルが読めない、または原典が基準と一致しない。"""


@dataclass(frozen=True)
class Baseline:
    tag: str
    commit: str
    tag_object: str | None
    decided_at: str | None
    historical: dict[str, str]

    def known_tags(self) -> set[str]:
        return {self.tag, *self.historical}

    def historical_tokens(self) -> set[str]:
        """現役の経路に残ってはいけない文字列（過去のタグと commit）。"""
        return set(self.historical) | set(self.historical.values())


# 原典の置き場所の既定（cwd 相対）。tools/pekko-source/prepare_pekko_source.sh が用意する。
DEFAULT_SOURCE_ROOT = "pekko"


def source_root() -> Path:
    """全ステップが共有する原典の場所。PEKKO_ORIGINAL_ROOT があればそれ、無ければ pekko/。"""
    return Path(os.environ.get("PEKKO_ORIGINAL_ROOT") or DEFAULT_SOURCE_ROOT)


def baseline_file() -> Path:
    return Path(os.environ.get("PEKKO_BASELINE_FILE") or DEFAULT_BASELINE_FILE)


def load(path: Path | None = None) -> Baseline:
    path = baseline_file() if path is None else path
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise BaselineError(f"{path}: {error}") from error
    values: dict[str, str] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition("=")
        if not sep:
            raise BaselineError(f"{path}: not KEY=VALUE: {line!r}")
        values[key.strip()] = value.strip()
    tag = values.get("PEKKO_BASELINE_TAG")
    commit = values.get("PEKKO_BASELINE_COMMIT")
    if not tag or not commit or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise BaselineError(f"{path}: PEKKO_BASELINE_TAG and a 40-hex PEKKO_BASELINE_COMMIT are required")
    historical: dict[str, str] = {}
    # 区切りはカンマ。空白で区切ると、シェルで source したときに 2 項目目がコマンドになる。
    for item in filter(None, values.get("PEKKO_HISTORICAL_BASELINES", "").split(",")):
        old_tag, sep, old_commit = item.partition(":")
        if not sep or not old_tag or not re.fullmatch(r"[0-9a-f]{40}", old_commit):
            raise BaselineError(f"{path}: PEKKO_HISTORICAL_BASELINES item must be <tag>:<commit>: {item!r}")
        if old_tag == tag:
            raise BaselineError(f"{path}: {tag} is both current and historical")
        historical[old_tag] = old_commit
    return Baseline(tag, commit, values.get("PEKKO_BASELINE_TAG_OBJECT"),
                    values.get("PEKKO_BASELINE_DECIDED_AT"), historical)


def source_tag(baseline: Baseline, pekko_dir: str | Path) -> str:
    """pekko_dir が現行の基準 commit をそのままチェックアウトした原典なら、現行タグを返す。

    新しく green にするノードの照合済みタグはここで決める。次のどれかに当たれば
    BaselineError にする（旧原典や手を加えた原典での照合に現行タグを付けない）。
    - HEAD が現行の基準 commit でない
    - 追跡ファイルの変更（未 stage・stage 済み）、または無視されない未追跡ファイルがある
    - assume-unchanged か skip-worktree の印が付いたファイルがある（変更が status に出ない）
    原典の中身は変えない（利用者の変更を自動で消さない）。
    """
    pekko_dir = Path(pekko_dir)
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    # replace ref で別の commit に見せかけられないようにする。
    env["GIT_NO_REPLACE_OBJECTS"] = "1"

    def git(*args: str) -> str:
        proc = subprocess.run(["git", "-C", str(pekko_dir), *args], env=env,
                              capture_output=True, text=True, check=False)
        if proc.returncode != 0:
            raise BaselineError(f"{pekko_dir}: git {' '.join(args)} failed: {proc.stderr.strip()}")
        return proc.stdout.strip()

    if not pekko_dir.is_dir():
        raise BaselineError(f"{pekko_dir}: the Pekko source is not prepared")
    # 外側の repo の HEAD を拾わないよう、pekko_dir 自身が toplevel であることを確かめる。
    toplevel = git("rev-parse", "--show-toplevel")
    if Path(toplevel).resolve() != pekko_dir.resolve():
        raise BaselineError(f"{pekko_dir}: not the toplevel of its own git repository")
    head = git("rev-parse", "--verify", "HEAD^{commit}")
    if head != baseline.commit:
        old = [t for t, c in baseline.historical.items() if c == head]
        known = f" (historical baseline {old[0]})" if old else ""
        raise BaselineError(
            f"{pekko_dir}: HEAD is {head}{known}, not the current baseline {baseline.tag} {baseline.commit}"
        )
    changes = git("status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none").splitlines()
    if changes:
        shown = "; ".join(c.strip() for c in changes[:5])
        raise BaselineError(
            f"{pekko_dir}: {len(changes)} local change(s) on top of {baseline.tag} ({shown}); "
            "the source must be an unmodified checkout (not removed automatically)"
        )
    hidden = [line for line in git("ls-files", "-v").splitlines()
              if line[:1].islower() or line[:1] == "S"]
    if hidden:
        raise BaselineError(
            f"{pekko_dir}: {len(hidden)} file(s) marked assume-unchanged or skip-worktree "
            f"({'; '.join(h[2:] for h in hidden[:5])}); their changes are not visible to git status"
        )
    return baseline.tag


def check_nodes(nodes: list[dict], baseline: Baseline) -> list[str]:
    """台帳ノードの verified_against を検査し、違反の一覧を返す。"""
    errors = []
    known = baseline.known_tags()
    for node in nodes:
        node_id = node.get("id")
        status = node.get("status")
        value = node.get(FIELD)
        if value is not None and not isinstance(value, str):
            errors.append(f"{node_id}: {FIELD} must be a tag string or null, got {value!r}")
            continue
        if status in JUDGED_STATUSES:
            if value is None:
                errors.append(f"{node_id}: status {status} needs {FIELD} (one of {sorted(known)})")
            elif value not in known:
                errors.append(f"{node_id}: {FIELD} {value!r} is not a known baseline tag {sorted(known)}")
        elif value is not None:
            errors.append(f"{node_id}: status {status} must not carry {FIELD} ({value!r})")
    return errors


def summarize(nodes: list[dict], baseline: Baseline) -> dict:
    by_tag = collections.Counter(
        (n.get("status"), n.get(FIELD)) for n in nodes if n.get("status") in JUDGED_STATUSES
    )
    unverified = [n for n in nodes
                  if n.get("status") in JUDGED_STATUSES and n.get(FIELD) != baseline.tag]
    return {"by_tag": by_tag, "unverified": unverified}


def _load_nodes(path: str) -> list[dict]:
    import yaml

    with open(path, encoding="utf-8") as f:
        return (yaml.safe_load(f) or {}).get("nodes", [])


def cmd_source_tag(args) -> int:
    print(source_tag(load(), args.pekko or source_root()))
    return 0


def cmd_source_root(args) -> int:
    root = source_root()
    source_tag(load(), root)
    print(root.resolve())
    return 0


def cmd_check_ledger(args) -> int:
    baseline = load()
    nodes = _load_nodes(args.ledger)
    errors = check_nodes(nodes, baseline)
    summary = summarize(nodes, baseline)
    print(f"基準: 現行 {baseline.tag}（{baseline.commit}）、過去 {sorted(baseline.historical) or '-'}")
    print(f"判定済みノード（{'/'.join(JUDGED_STATUSES)}）の照合済みタグ:")
    for (status, tag), count in sorted(summary["by_tag"].items(), key=lambda x: (str(x[0][0]), str(x[0][1]))):
        print(f"  {status:12s} {str(tag):10s} {count:5d}")
    unverified = summary["unverified"]
    by_status = collections.Counter(n.get("status") for n in unverified)
    print(f"{baseline.tag} で未照合: {len(unverified)} 件 {dict(sorted(by_status.items()))}")
    if args.list_unverified:
        for n in unverified:
            print(f"  - {n['id']} ({n.get('status')}, {FIELD}={n.get(FIELD)})")
    if args.pekko:
        try:
            source_tag(baseline, args.pekko)
        except BaselineError as error:
            print(error, file=sys.stderr)
            return 1
        missing = [n for n in nodes if not (Path(args.pekko) / n["id"]).is_file()]
        missing_by_status = collections.Counter(n.get("status") for n in missing)
        print(f"{baseline.tag} の原典にファイルが無いノード: {len(missing)} 件 {dict(sorted(missing_by_status.items()))}")
        if args.list_unverified:
            for n in missing:
                print(f"  - {n['id']} ({n.get('status')})")
    if errors:
        print(f"違反 {len(errors)} 件:", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print("verified_against: OK")
    return 0


def _active_files(root: Path) -> list[Path]:
    """現役の経路のうち、git が扱うファイル（追跡済みと、無視されない未追跡）を返す。

    .takt/runs/ など git が無視する生成物（過去の run の記録）は対象にしない。
    """
    proc = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard",
         "--", *ACTIVE_PATHS],
        env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
        capture_output=True, check=False,
    )
    if proc.returncode != 0:
        raise BaselineError(f"{root}: git ls-files failed: {proc.stderr.decode(errors='replace').strip()}")
    history = set(HISTORY_TOOLS)
    names = sorted({n for n in proc.stdout.decode("utf-8").split("\0") if n})
    return [root / n for n in names if n not in history and (root / n).is_file()]


def find_stale_refs(root: Path, baseline: Baseline) -> list[str]:
    """現役の経路で過去の基準タグや commit を書いた行を返す。

    凍結した過去の基準のディレクトリ（docs/baseline/pekko-<タグ>/）を指すパスは履歴への
    参照なので許す。
    """
    tokens = sorted(baseline.historical_tokens())
    if not tokens:
        return []
    pattern = re.compile("|".join(re.escape(t) for t in tokens))
    history_dirs = re.compile("|".join(rf"docs/baseline/pekko-{re.escape(t)}/" for t in baseline.historical))
    hits = []
    for path in _active_files(root):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if pattern.search(history_dirs.sub("", line)):
                hits.append(f"{path.relative_to(root)}:{number}: {line.strip()}")
    return hits


def cmd_check_refs(args) -> int:
    baseline = load()
    hits = find_stale_refs(Path(args.root), baseline)
    if hits:
        print(f"現役の経路に過去の基準 {sorted(baseline.historical)} が残っている（{len(hits)} 行）:", file=sys.stderr)
        for h in hits:
            print(f"  {h}", file=sys.stderr)
        return 1
    print(f"現役の経路に過去の基準 {sorted(baseline.historical)} の参照なし（基準 {baseline.tag}）")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="command", required=True)
    p = sub.add_parser("source-tag", help="原典が変更のない現行の基準なら、その基準タグを出す")
    p.add_argument("--pekko", default=None, help="既定は PEKKO_ORIGINAL_ROOT、無ければ pekko")
    p.set_defaults(func=cmd_source_tag)
    p = sub.add_parser("source-root", help="全ステップが使う原典を検査し、その絶対パスを出す")
    p.set_defaults(func=cmd_source_root)
    p = sub.add_parser("check-ledger", help="台帳の verified_against を検査して集計する")
    p.add_argument("--ledger", default="ledger/nodes.yaml")
    p.add_argument("--pekko", default=None, help="現行の原典。ノードのファイルの有無も数える")
    p.add_argument("--list-unverified", action="store_true")
    p.set_defaults(func=cmd_check_ledger)
    p = sub.add_parser("check-refs", help="現役の経路に過去の基準が残っていないか調べる")
    p.add_argument("--root", default=str(REPO))
    p.set_defaults(func=cmd_check_refs)
    args = ap.parse_args()
    try:
        return args.func(args)
    except BaselineError as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
