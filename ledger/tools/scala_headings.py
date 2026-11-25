#!/usr/bin/env python3
"""Scala の WordSpec の見出しを数える（verify_red の原典のケース数の数え方から使う）。

見出し（`"name" must {` など）はケースではなく、下のケースをまとめるブロックである。
"""
import re

# Scala のケース書式。`"..." in {` の前に taggedAs が付く形も同じケースである。
# `it should "..."` の書式も数える。verify_red の原典の件数と、見出しの中のケースの判定が使う。
SCALA_CASE = re.compile(
    r"""^\s*(?:
        it\s+should\s+"[^"]*"\s+in\s*\{
      | "[^"]*"(?:\s+taggedAs\b[^\n{]*?)?\s+in\s*\{
    )""",
    re.MULTILINE | re.VERBOSE,
)

# WordSpec の見出し（`"name" must {` など）。ケースではなく、下のケースをまとめるブロックである。
# 見出しとして認めるのは、波括弧で閉じたブロックの中に既知の書式のケースが 1 件以上あるときだけ
# （空の見出しや、未知の書式だけを持つ見出しは、件数を確かめられないので数えられない扱いにする）。
SCALA_HEADING = re.compile(
    r'^\s*"[^"]*"\s+(?:must|should|can|when|which)\s*\{', re.MULTILINE)


def _block_end(text: str, open_brace: int) -> int | None:
    """text[open_brace] の `{` に対応する `}` の位置を返す。文字列とコメントの中の括弧は数えない。"""
    depth, i, n = 0, open_brace, len(text)
    while i < n:
        if text.startswith('"""', i):
            end = text.find('"""', i + 3)
            if end < 0:
                return None
            i = end + 3
            continue
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"' and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            i = j + 1
            continue
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end < 0 else end
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            if end < 0:
                return None
            i = end + 2
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def count_scala_headings(text: str) -> int | None:
    """WordSpec の見出しの数を返す。既知のケースを持たない見出しがあれば None。"""
    count = 0
    for m in SCALA_HEADING.finditer(text):
        end = _block_end(text, m.end() - 1)
        if end is None or not SCALA_CASE.search(text, m.end(), end):
            return None
        count += 1
    return count
