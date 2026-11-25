#!/usr/bin/env python3
"""Scala の原典のケースを数える（verify_red の原典のケース数の数え方から使う）。

ケース、WordSpec の見出し（`"name" must {` など）、未知の宣言を、同じ字句処理で見分ける。
`mask` がコメントを空白に、文字列と文字の中身を `x` に置き換えたテキストを作り（長さと改行の
位置は元と同じ）、以下の正規表現と波括弧の対応はすべてその上で読む。コメントの中のケースは
数えず、エスケープした引用符や文字列の中の波括弧で読み違えない。
"""
import re

# Scala のケース書式。`"..." in {` の前に taggedAs が付く形も同じケースである。
# `it should "..."` の書式も数える。
SCALA_CASE = re.compile(
    r"""^\s*(?:
        it\s+should\s+"[^"]*"\s+in\s*\{
      | "[^"]*"(?:\s+taggedAs\b[^\n{]*?)?\s+in\s*\{
    )""",
    re.MULTILINE | re.VERBOSE,
)
# 文字列リテラルで始まる宣言の候補。ケースと見出しは必ずここにも数えられる。どちらでもない
# 候補（`"x" in new Fixture {`、`"x".in { }` など）があれば、件数を確かめられないと判断する。
# `it should "..."` で始まる行も、SCALA_CASE と同じ行を両側で数えるために含める。
SCALA_NAMED_PREFIX = re.compile(
    r'^\s*(?:it\s+should\s+)?"[^"]+"(?:\s+(?=\S)|\s*\.)', re.MULTILINE)
# WordSpec の見出し。ケースではなく、下のケースをまとめるブロックである。見出しとして認めるのは、
# 波括弧で閉じたブロックの中に既知の書式のケースが 1 件以上あるときだけ（空の見出しや、未知の
# 書式だけを持つ見出しは、件数を確かめられないので数えられない扱いにする）。
SCALA_HEADING = re.compile(
    r'^\s*"[^"]*"\s+(?:must|should|can|when|which)\s*\{', re.MULTILINE)


def mask(text: str) -> str | None:
    """コメントを空白に、文字列と文字の中身を `x` に置き換える。閉じていなければ None。

    行コメント、入れ子の複数行コメント、`"..."`（バックスラッシュのエスケープ）、
    `\"\"\"...\"\"\"`、文字リテラル（`'"'`、`'\\''` など）を扱う。改行は残す。
    """
    out = list(text)
    i, n = 0, len(text)

    def blank(start: int, end: int, keep_ends: bool) -> None:
        for k in range(start, end):
            if text[k] == "\n":
                continue
            if keep_ends and k in (start, end - 1):
                continue
            out[k] = " " if not keep_ends else "x"

    while i < n:
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            blank(i, end, keep_ends=False)
            i = end
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                return None
            blank(i, j, keep_ends=False)
            i = j
        elif text.startswith('"""', i):
            end = text.find('"""', i + 3)
            if end < 0:
                return None
            while text.startswith('"', end + 3):
                end += 1
            blank(i, end + 3, keep_ends=True)
            i = end + 3
        elif text[i] == '"':
            j = i + 1
            while j < n and text[j] != '"':
                if text[j] == "\n":
                    return None
                j += 2 if text[j] == "\\" else 1
            if j >= n:
                return None
            blank(i, j + 1, keep_ends=True)
            i = j + 1
        elif text[i] == "'":
            m = re.match(r"'(?:\\.|[^'\\\n])'", text[i:])
            if m:
                blank(i, i + m.end(), keep_ends=True)
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def _block_end(masked: str, open_brace: int) -> int | None:
    """masked[open_brace] の `{` に対応する `}` の位置を返す（mask 済みのテキストを読む）。"""
    depth = 0
    for i in range(open_brace, len(masked)):
        if masked[i] == "{":
            depth += 1
        elif masked[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    return None


def count_scala_headings(masked: str) -> int | None:
    """WordSpec の見出しの数を返す。既知のケースを持たない見出しがあれば None。"""
    count = 0
    for m in SCALA_HEADING.finditer(masked):
        end = _block_end(masked, m.end() - 1)
        if end is None or not SCALA_CASE.search(masked, m.end(), end):
            return None
        count += 1
    return count


def count_scala_cases(text: str) -> int | None:
    """Scala の原典のケース数を返す。既知の書式で全件を説明できなければ None。

    文字列リテラルで始まる宣言の候補の数が、ケースと見出しの数の和に一致することを確かめる。
    """
    masked = mask(text)
    if masked is None:
        return None
    cases = len(SCALA_CASE.findall(masked))
    headings = count_scala_headings(masked)
    if headings is None or len(SCALA_NAMED_PREFIX.findall(masked)) != cases + headings:
        return None
    return cases
