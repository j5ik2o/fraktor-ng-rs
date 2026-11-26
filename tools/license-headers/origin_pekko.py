"""#206 の線引きの原典側：Pekko v2.0.0 の索引と、ヘッダーへ写す原典の表示。

ヘッダーへ写すのは、原典のファイルの package 文より前にあるコメント（ASF の表示、
Lightbend の表示、第三者の著作権表示）で、文面は変えない（docs/design/issue-206-license-policy.md §3.1）。
"""
import re
import shutil
import subprocess
import tempfile
from collections import defaultdict
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path

from notice_render import (
    PEKKO_COMMIT,
    leading_blocks,
    third_party_blocks_from_tree,
)
from origin_evidence import MIRRORS, TEXT_THIRD_PARTY, best_path

# 原典のファイルを引く記述（ヘッダーに写す表示の出どころを決めるため）。
PEKKO_FILE = re.compile(r"\b[\w-]+(?:/[\w-]+)*/src/[\w/.-]+?\.(?:scala|java)\b")

PEKKO_FQN = re.compile(r"\borg\.apache\.pekko\.((?:[a-z_][\w]*\.)*)([A-Z]\w*)")

BACKTICK_TYPE = re.compile(r"`([A-Z][A-Za-z0-9]*)(?:\.[A-Za-z]\w*)*(?:\([^`]*\))?`")

# 原典のファイル先頭の表示のうち、ASF と Lightbend（Typesafe）以外のもの。
#（Scala の「Copyright EPFL and Lightbend, Inc.」は第三者の表示なので、Lightbend の語だけでは外さない）
OWN_NOTICE = re.compile(
    r"Copyright \(C\) [\d\s-]+(?:Lightbend|Typesafe) Inc\.|Apache Software Foundation|copyright ownership"
)

COPYRIGHT = re.compile(r"\bCopyright\b")

# 第三者の表示を原典のファイル先頭に持つ Pekko のファイルを、fraktor の本文が型名で引いているか。
# Helpers.scala は `Requiring` だけが scala-library 由来なので、その名前で見る。
THIRD_PARTY_NAME_OVERRIDE = {"actor/src/main/scala/org/apache/pekko/util/Helpers.scala": "Requiring"}

DECL = re.compile(
    r"^\s*(?:(?:final|sealed|abstract|case|implicit|private\[[\w.]+\]|protected\[[\w.]+\])\s+)*"
    r"(?:class|trait|object|interface|enum|record)\s+([A-Z][A-Za-z0-9_]*)",
    re.MULTILINE,
)


def pekko_paths(git_repo):
    out = subprocess.run(
        ["git", "-C", str(git_repo), "ls-tree", "-r", "--name-only", PEKKO_COMMIT],
        check=True, capture_output=True, text=True,
    )
    return [line for line in out.stdout.splitlines() if line]

TREE_STAMP = ".pekko-commit"


def pekko_tree(git_repo, target=None):
    """原典の tree を展開して、そのパスを返す（読むだけ。フェッチはしない）。

    戻り値は (展開先, 一時ディレクトリを作ったか)。呼び出し側が後始末に使う。
    `target` を渡すと、固定コミット専用の展開先として使う。展開したときに `.pekko-commit`
    へ固定コミットを書き、次からはそれが一致するときだけ再利用する。別のリビジョンの作業
    ツリーや、印の無い空でないディレクトリは使わずに止める（中身は消さない）。
    """
    temporary = target is None
    if target is None:
        target = Path(tempfile.mkdtemp(prefix="pekko-v2.0.0-"))
    else:
        stamp = target / TREE_STAMP
        if stamp.is_file() and stamp.read_text(encoding="utf-8").strip() == PEKKO_COMMIT:
            return target, False
        if target.exists() and any(target.iterdir()):
            raise SystemExit(
                f"--pekko-tree {target} は固定コミット {PEKKO_COMMIT} の展開先ではない"
                f"（{TREE_STAMP} が無いか違う）。空のディレクトリか、新しいパスを指定する"
            )
        target.mkdir(parents=True, exist_ok=True)
    try:
        archive = subprocess.run(
            ["git", "-C", str(git_repo), "archive", PEKKO_COMMIT], check=True, capture_output=True
        )
        subprocess.run(["tar", "-x", "-C", str(target)], input=archive.stdout, check=True)
    except BaseException:
        if temporary:
            shutil.rmtree(target, ignore_errors=True)
        raise
    if not temporary:
        (target / TREE_STAMP).write_text(PEKKO_COMMIT + "\n", encoding="utf-8")
    return target, temporary


def pekko_index(paths):
    """原典のファイル名（拡張子抜き、小文字）から原典パスへの索引。"""
    index = defaultdict(list)
    for path in paths:
        if not path.endswith((".scala", ".java")):
            continue
        stem = Path(path).stem.lower()
        index[stem].append(path)
    return index

def pekko_types(tree):
    """原典の型名から原典パスへの索引（同じ名前の宣言が多いものは使わない）。"""
    index = defaultdict(list)
    for path in tree.rglob("*"):
        if path.suffix not in {".scala", ".java"} or not path.is_file():
            continue
        rel = str(path.relative_to(tree))
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            found = DECL.match(line)
            if found:
                index[found.group(1)].append(rel)
    return index

def fold_types(types):
    """型名を小文字にそろえた索引（H1 の規則 1）。"""
    folded = defaultdict(list)
    for name, paths in types.items():
        folded[name.lower()].extend(paths)
    return folded

def resolve_fqn(pkgs, name, types, stems):
    """`org.apache.pekko.<pkgs>.<Name>` を原典のファイルに引く。"""
    package_dir = "org/apache/pekko/" + pkgs.replace(".", "/")
    hits = [path for path in types.get(name, []) if package_dir in path]
    if not hits:
        hits = [path for path in stems.get(name.lower(), []) if package_dir in path]
    return best_path(hits) if hits else None

def source_files(text, ledger_hit, evidence_paths, pekko):
    """ファイルが写した原典のファイル（ヘッダーに表示を写す元）を、強い順に返す。

    1. 本文に書かれた原典のパス（移植マーカーの次の行など）
    2. 台帳の id（原典のパス）
    3. `Mirrors` の行と、本文の `org.apache.pekko.X` が指す型のファイル
    4. 判定の根拠（原典同名・型名・`Type.method`）が指すファイル
    """
    types, stems, pekko_set = pekko.types, pekko.stems, pekko.paths
    found = []

    def add(path):
        if path and path in pekko_set and path not in found:
            found.append(path)

    for match in PEKKO_FILE.finditer(text):
        candidate = match.group(0)
        if candidate in pekko_set:
            add(candidate)
        else:
            tail = candidate.split("/src/", 1)[-1]
            for path in stems.get(Path(candidate).stem.lower(), []):
                if path.endswith(tail):
                    add(path)
    if ledger_hit:
        add(ledger_hit[0])
    mirrors_lines = [line for line in text.splitlines() if MIRRORS.search(line)]
    others = [line for line in text.splitlines() if not MIRRORS.search(line)]
    for line in mirrors_lines + others:
        for match in PEKKO_FQN.finditer(line):
            add(resolve_fqn(match.group(1).rstrip("."), match.group(2), types, stems))
    # `Mirrors` の行の `` `Type` `` / `` `Type.member` ``（パッケージを書いていないもの）。
    for line in mirrors_lines:
        for match in BACKTICK_TYPE.finditer(line):
            hits = types.get(match.group(1), [])
            if hits and len(hits) <= 6:
                add(best_path(hits))
    for path in evidence_paths:
        add(path)
    return found

def third_party_notices(tree, pekko_paths_list):
    """原典のファイル先頭に、ASF と Lightbend 以外の Copyright 行があるファイル。"""
    notices = {}
    for path in pekko_paths_list:
        if not path.endswith((".scala", ".java")) or path.startswith(("project/", "docs/")):
            continue
        lines = leading_blocks((tree / path).read_text(encoding="utf-8", errors="replace"))
        owners = [line.strip() for line in lines if COPYRIGHT.search(line) and not OWN_NOTICE.search(line)]
        if owners:
            notices[path] = owners
    return notices

def third_party_evidence(text, sources, notices):
    """ヘッダーに写す第三者の表示。原典のファイル先頭の表示と、本文の名前で決める。"""
    found = []
    for path, owners in sorted(notices.items()):
        name = THIRD_PARTY_NAME_OVERRIDE.get(path, Path(path).stem)
        # 前方一致で見る（`DnsSettingsValues` のように、原典の型名から作った Rust の型名も拾う）。
        if path in sources or re.search(rf"\b{re.escape(name)}\w*", text):
            for owner in owners:
                label = f"{owner}（{Path(path).name}）"
                if label not in found:
                    found.append(label)
    for pattern, label in TEXT_THIRD_PARTY:
        if pattern.search(text) and label not in found:
            found.append(label)
    return found


@dataclass(frozen=True)
class PekkoIndex:
    """原典の索引。判定（origin_evidence.classify）とヘッダーの出どころ（source_files）が使う。"""

    paths: frozenset
    stems: dict
    types: dict
    folded: dict
    notices: dict
    tree: Path | None = None
    third_party_blocks: dict = field(default_factory=dict)


@contextmanager
def open_pekko(git_repo, tree_dir=None):
    """原典の git リポジトリから索引を作る。一時の展開先は、例外のときも抜けるときに消す
    （利用者が `--pekko-tree` で渡した展開先は消さない）。"""
    all_paths = pekko_paths(git_repo)
    tree, temporary = pekko_tree(git_repo, tree_dir)
    try:
        types = pekko_types(tree)
        notices = third_party_notices(tree, all_paths)
        yield PekkoIndex(
            paths=frozenset(all_paths),
            stems=pekko_index(all_paths),
            types=types,
            folded=fold_types(types),
            notices=notices,
            tree=tree,
            third_party_blocks=third_party_blocks_from_tree(tree, notices),
        )
    finally:
        if temporary:
            shutil.rmtree(tree, ignore_errors=True)
