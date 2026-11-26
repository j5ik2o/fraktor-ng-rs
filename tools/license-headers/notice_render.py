"""#206 原典（Pekko v2.0.0）コメントブロックの抽出とヘッダー表示の組み立て（§3.1）。"""
import subprocess
from pathlib import Path

from header_core import HASH_PROSPECTOR_LABEL, SCALA_LIBRARY_LABEL

PEKKO_COMMIT = "9fbd2283b1cf701c90a1223bf621942c191db61f"

DEFAULT_PEKKO_BLOCKS = (
    (
        "Licensed to the Apache Software Foundation (ASF) under one or more",
        "license agreements; and to You under the Apache License, version 2.0:",
        "",
        "  https://www.apache.org/licenses/LICENSE-2.0",
        "",
        "This file is part of the Apache Pekko project, which was derived from Akka.",
    ),
    ("Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>",),
)

# 原典 Helpers.scala 18〜25 行と同じ文面（§2.1 の 2）。
SCALA_LIBRARY_BLOCK = (
    "Scala (https://www.scala-lang.org)",
    "",
    "Copyright EPFL and Lightbend, Inc.",
    "",
    "Licensed under Apache License 2.0",
    "(http://www.apache.org/licenses/LICENSE-2.0).",
    "",
    "See the NOTICE file distributed with this work for",
    "additional information regarding copyright ownership.",
)


def comment_blocks(source_text):
    """原典ファイルの package 文より前のコメントを、ブロックごとの行タプルで返す（§3.1）。"""
    blocks = []
    current = None
    for line in source_text.splitlines():
        stripped = line.strip()
        if current is None and stripped.startswith("package "):
            break
        if current is None and stripped.startswith("/*"):
            current = []
            body = stripped[2:].lstrip("*").strip()
            if body:
                current.append(body)
            continue
        if current is not None:
            if stripped.endswith("*/"):
                body = stripped[:-2].rstrip()
                body = body[1:] if body.startswith("*") else body
                if body.strip():
                    current.append(body.strip())
                if current:
                    blocks.append(tuple(current))
                current = None
                continue
            current.append(stripped[2:] if stripped.startswith("* ") else stripped.lstrip("*"))
            continue
        if stripped.startswith("//"):
            blocks.append((stripped,))
    return blocks


def leading_blocks(source_text):
    """原典のファイルの、package 文より前にあるコメントを、文面を変えずに行で返す。"""
    lines = []
    for block in comment_blocks(source_text):
        lines.extend(block)
        if not (len(block) == 1 and block[0].startswith("//")):
            lines.append("")
    return lines


def format_notice_blocks(blocks):
    """コメントブロック列を、Rust の `//` コメント行のタプルへ整形する（§3.1）。"""
    lines = []
    for idx, block in enumerate(blocks):
        if idx > 0:
            lines.append("//")
        for line in block:
            if line == "":
                lines.append("//")
            elif line.startswith("//") and not line.startswith(("///", "//!")):
                lines.append(line)
            else:
                lines.append(f"// {line}")
    return tuple(lines)


def blocks_for_owner(source_text, owner):
    """原典コメントブロックのうち、指定された第三者 Copyright 行を含むブロックを返す。"""
    return [block for block in comment_blocks(source_text) if any(owner in line for line in block)]


def third_party_blocks_from_tree(tree, notices):
    """`(owner, filename)` から原典の第三者コメントブロック列への辞書を作る。"""
    result = {}
    for path, owners in notices.items():
        text = (tree / path).read_text(encoding="utf-8", errors="replace")
        filename = Path(path).name
        for owner in owners:
            blocks = blocks_for_owner(text, owner)
            if blocks:
                result[(owner, filename)] = blocks
    return result


def render_pekko_notice_lines(sources, third_party_labels, read_source, resolve_third_party_blocks):
    """原典ファイル一覧と第三者表示ラベルから、ヘッダーへ写す表示コメント行を組み立てる（§3.1）。"""
    blocks = []

    def append_unique(block):
        if block not in blocks:
            blocks.append(block)

    if sources:
        for src in sources:
            for block in comment_blocks(read_source(src)):
                append_unique(block)
    else:
        for block in DEFAULT_PEKKO_BLOCKS:
            append_unique(block)

    for label in third_party_labels:
        if label == HASH_PROSPECTOR_LABEL:
            continue
        if label == SCALA_LIBRARY_LABEL:
            append_unique(SCALA_LIBRARY_BLOCK)
            continue
        if label.endswith((".scala）", ".java）")):
            owner, rest = label.rsplit("（", 1)
            filename = rest[:-1]
            for block in resolve_third_party_blocks(owner, filename):
                append_unique(block)
    return format_notice_blocks(blocks)


class PekkoGitReader:
    """原典 git リポジトリの固定コミットからファイルをオンデマンド読み取りするリーダー。"""

    def __init__(self, git_repo):
        self.git_repo = Path(git_repo)
        out = subprocess.run(
            ["git", "-C", str(self.git_repo), "ls-tree", "-r", "--name-only", PEKKO_COMMIT],
            check=True,
            capture_output=True,
            text=True,
        )
        self.paths = [line for line in out.stdout.splitlines() if line]
        self.path_set = frozenset(self.paths)
        self._cache = {}
        self._third_party_cache = {}

    def read_source(self, rel_path):
        if rel_path not in self.path_set:
            raise FileNotFoundError(f"upstream Pekko source not found at {PEKKO_COMMIT}: {rel_path}")
        if rel_path not in self._cache:
            out = subprocess.run(
                ["git", "-C", str(self.git_repo), "show", f"{PEKKO_COMMIT}:{rel_path}"],
                check=True,
                capture_output=True,
                text=True,
            )
            self._cache[rel_path] = out.stdout
        return self._cache[rel_path]

    def third_party_blocks(self, owner, filename):
        key = (owner, filename)
        if key not in self._third_party_cache:
            found = []
            for path in self.paths:
                if Path(path).name != filename or path.startswith(("project/", "docs/")):
                    continue
                for block in blocks_for_owner(self.read_source(path), owner):
                    if block not in found:
                        found.append(block)
            if not found:
                raise FileNotFoundError(f"third-party notice block not found: {owner} ({filename})")
            self._third_party_cache[key] = found
        return self._third_party_cache[key]
