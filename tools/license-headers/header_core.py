"""#206 ヘッダー処理の純粋関数群（構造化ブロック認識・適用・除去）。

ファイルシステムや git に依存しない純粋なテキスト変換と解析だけを持つ。
仕様: docs/design/issue-206-license-policy.md §3.1, §3.4.3, §7.1。
"""
import datetime
import hashlib
import re
from dataclasses import dataclass

SPDX_APACHE = "Apache-2.0"
SPDX_DUAL = "MIT OR Apache-2.0"
HELD = "保留"

# 決定の式の文法（設計レビュー 3 回目の M2）。
EXTERNAL_IDS = ("BSD-2-Clause", "BSD-3-Clause", "CC0-1.0", "Unlicense")
EXTERNAL_EXPRESSIONS = frozenset(f"{SPDX_APACHE} AND {item}" for item in EXTERNAL_IDS)

MIT_TOKEN = re.compile(r"\bmit\b", re.IGNORECASE)

# 厳密な移植マーカー（F2: 大文字小文字を区別し、語の境界を見る）と Mirrors（本文全体）。
PORTED_MARKER = re.compile(r"\b(Ported from|Port of|Checked against) Pekko\b")
MIRRORS_PEKKO = re.compile(r"Mirrors `org\.apache\.pekko\.")
MARKER_WINDOW = 20

# ヘッダーブロックの認識用正規表現（§3.4.3）。
SPDX_LINE = re.compile(r"^// SPDX-License-Identifier:\s*(.+?)\s*$")
ANY_SPDX_TOKEN = re.compile(r"SPDX-License-Identifier:")
OTHER_SPDX_LINE = re.compile(
    r"^\s*(?://|#|/\*|\*|<!--)?\s*SPDX-License-Identifier:\s*([^\s`].*?)\s*(?:\*/|-->)?\s*$"
)
REGULAR_COMMENT = re.compile(r"^//($|[^/!].*)")
RUST_COMMENT_LINE = re.compile(r"^\s*(?://|/\*|\*)")

# 終端アンカー（L2: 固定する。変えるときは旧アンカーを一覧に残す）。
PEKKO_ANCHOR = "// This file has been modified from the Apache Pekko original."
PEKKO_ANCHORS = (PEKKO_ANCHOR,)
ORIGINAL_ANCHOR = "// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root."
ORIGINAL_ANCHORS = (ORIGINAL_ANCHOR,)

PEKKO_TRAILER_LINES = (
    "//",
    "// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE",
    "// at the repository root.",
    "//",
    PEKKO_ANCHOR,
)

# 原典ファイルが決まらない場合の既定表示（§3.1）。
DEFAULT_PEKKO_NOTICE_LINES = (
    "// Licensed to the Apache Software Foundation (ASF) under one or more",
    "// license agreements; and to You under the Apache License, version 2.0:",
    "//",
    "//   https://www.apache.org/licenses/LICENSE-2.0",
    "//",
    "// This file is part of the Apache Pekko project, which was derived from Akka.",
    "//",
    "// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>",
)

SCALA_LIBRARY_LABEL = "Scala（scala-library）: Copyright EPFL and Lightbend, Inc."
HASH_PROSPECTOR_LABEL = "hash-prospector: public domain（Unlicense）"

ORIGINAL_COPYRIGHT_HOLDER = "Junichi Kato"
MAX_HEADER_LINE_LEN = 100


class HeaderError(ValueError):
    """ヘッダーの解析・適用・除去で拒否された入力のエラー。"""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(frozen=True)
class ParsedHeader:
    state: str  # NONE | APACHE | DUAL | EXTERNAL | UNRECOGNIZED
    spdx: str | None
    anchor_kind: str | None  # "pekko" | "original" | None
    bom: str
    shebang: str
    header_lines: tuple[str, ...]
    has_separator_blank: bool
    body: str


def valid_expression(spdx):
    """決定の式が 3 つの形のどれかと、綴りまで一致するか。"""
    return spdx in (SPDX_APACHE, SPDX_DUAL) or spdx in EXTERNAL_EXPRESSIONS


def mentions_mit(spdx):
    """`MIT` を含む式は、綴りや並びによらず、すべて MIT として扱う（M2）。"""
    return bool(MIT_TOKEN.search(spdx or ""))


def split_lines_strict(text):
    """\\r\\n と \\n だけを改行として `(行本体, 改行記号)` のリストを返す。"""
    if not text:
        return []
    lines = []
    pos = 0
    length = len(text)
    while pos < length:
        lf = text.find("\n", pos)
        next_crlf = -1
        search_cr = text.find("\r", pos)
        while search_cr != -1:
            if search_cr + 1 < length and text[search_cr + 1] == "\n":
                next_crlf = search_cr
                break
            search_cr = text.find("\r", search_cr + 1)
        if next_crlf != -1 and (lf == -1 or next_crlf < lf):
            lines.append((text[pos:next_crlf], "\r\n"))
            pos = next_crlf + 2
        elif lf != -1:
            lines.append((text[pos:lf], "\n"))
            pos = lf + 1
        else:
            lines.append((text[pos:], ""))
            break
    return lines


def newline_of(text):
    """ファイルが CRLF を含むなら \\r\\n、そうでなければ \\n を返す。"""
    return "\r\n" if "\r\n" in text else "\n"


def split_prefix(text):
    """UTF-8 BOM と rustc 互換の shebang を先頭から分離する（§3.4.3）。"""
    bom = ""
    rest = text
    if rest.startswith("\ufeff"):
        bom = "\ufeff"
        rest = rest[1:]
    shebang = ""
    if rest.startswith("#!"):
        first_content, first_eol = split_lines_strict(rest)[0]
        if not first_content[2:].lstrip(" \t").startswith("["):
            if not first_eol:
                raise HeaderError("shebang-no-newline", "shebang line has no trailing newline")
            shebang = first_content + first_eol
            rest = rest[len(shebang):]
    return bom, shebang, rest


def parse_header(text, allowed_external=()):
    """テキスト先頭のライセンスヘッダーブロックを構造化解析する（§3.4.3）。"""
    bom, shebang, rest = split_prefix(text)
    lines = split_lines_strict(rest)
    if not lines:
        return ParsedHeader("NONE", None, None, bom, shebang, (), False, rest)
    first_match = SPDX_LINE.match(lines[0][0])
    if not first_match:
        return ParsedHeader("NONE", None, None, bom, shebang, (), False, rest)

    spdx = first_match.group(1)
    header_lines = []
    anchor_kind = None
    end_idx = 0
    for idx, (content, _eol) in enumerate(lines):
        if not REGULAR_COMMENT.match(content):
            break
        header_lines.append(content)
        end_idx = idx
        trimmed = content.rstrip()
        if trimmed in PEKKO_ANCHORS:
            anchor_kind = "pekko"
            break
        if trimmed in ORIGINAL_ANCHORS:
            anchor_kind = "original"
            break

    allowed = set(allowed_external)
    if anchor_kind == "pekko" and spdx == SPDX_APACHE:
        state = "APACHE"
    elif anchor_kind == "original" and spdx == SPDX_DUAL:
        state = "DUAL"
    elif spdx in allowed and (anchor_kind == "pekko" or anchor_kind is None):
        state = "EXTERNAL"
    else:
        state = "UNRECOGNIZED"

    consumed_end = end_idx + 1
    has_separator_blank = False
    if state != "UNRECOGNIZED" and consumed_end < len(lines):
        next_content, next_eol = lines[consumed_end]
        if next_content == "" and next_eol in ("\n", "\r\n"):
            has_separator_blank = True
            consumed_end += 1

    body = "".join(content + eol for content, eol in lines[consumed_end:])
    return ParsedHeader(state, spdx, anchor_kind, bom, shebang, tuple(header_lines), has_separator_blank, body)


def has_ported_marker(body):
    """本文の冒頭 20 行に厳密な移植マーカーがあるか（F2, I4）。"""
    return any(PORTED_MARKER.search(content) for content, _eol in split_lines_strict(body)[:MARKER_WINDOW])


def has_mirrors_pekko(body):
    r"""本文のどこかに `Mirrors `org.apache.pekko.` があるか（M1, C3）。"""
    return MIRRORS_PEKKO.search(body) is not None


def has_c3_pekko_marker(body):
    """C3 の矛盾検知に使う Pekko 由来マーカー判定。"""
    return has_ported_marker(body) or has_mirrors_pekko(body)


def notice_hash(notice_comment_lines):
    """写した表示ブロック（先頭 2 行と固定末尾 5 行を除いた行の並び）の SHA-256（C6）。"""
    return hashlib.sha256("\n".join(notice_comment_lines).encode("utf-8")).hexdigest()


def _validate_line_lengths(header_lines):
    for line in header_lines:
        if len(line) > MAX_HEADER_LINE_LEN:
            raise HeaderError("line-too-long", f"header line exceeds {MAX_HEADER_LINE_LEN} chars ({len(line)}): {line}")


def build_pekko_header_lines(spdx=SPDX_APACHE, notice_lines=None):
    """種別 1（Pekko 由来 / 外部式）のヘッダー行を組み立てる（§3.1）。"""
    body_lines = tuple(notice_lines) if notice_lines is not None else DEFAULT_PEKKO_NOTICE_LINES
    lines = (f"// SPDX-License-Identifier: {spdx}", "//") + body_lines + PEKKO_TRAILER_LINES
    _validate_line_lengths(lines)
    return lines


def build_original_header_lines(year=None):
    """種別 2（自作ファイル）のヘッダー行を組み立てる（§3.1）。"""
    resolved_year = datetime.date.today().year if year is None else year
    lines = (
        f"// SPDX-License-Identifier: {SPDX_DUAL}",
        "//",
        f"// Copyright (c) {resolved_year} {ORIGINAL_COPYRIGHT_HOLDER}",
        "//",
        "// Licensed under either of Apache License, Version 2.0 or MIT license",
        ORIGINAL_ANCHOR,
    )
    _validate_line_lengths(lines)
    return lines


def apply_header(text, kind, *, spdx=None, notice_lines=None, year=None, allowed_external=()):
    """ヘッダーを冪等に適用する（I1, I2, I3）。"""
    if kind not in ("pekko", "original", "external"):
        raise HeaderError("invalid-kind", f"unknown header kind: {kind!r}")
    parsed = parse_header(text, allowed_external=allowed_external)
    if ANY_SPDX_TOKEN.search(parsed.body):
        raise HeaderError("duplicate-spdx", "body already contains SPDX-License-Identifier:")
    if kind == "original" and has_c3_pekko_marker(parsed.body):
        raise HeaderError("origin-conflict", "cannot apply original header to file with Pekko marker")
    if parsed.state == "UNRECOGNIZED":
        raise HeaderError("unrecognized-header", "unrecognized SPDX header block")

    if kind == "pekko":
        target_spdx = spdx or SPDX_APACHE
        if parsed.state == "APACHE" and target_spdx == SPDX_APACHE:
            return text
        if parsed.state != "NONE":
            raise HeaderError("kind-conflict", f"conflicting existing header state: {parsed.state}")
        header_lines = build_pekko_header_lines(target_spdx, notice_lines)
    elif kind == "original":
        if parsed.state == "DUAL":
            return text
        if parsed.state != "NONE":
            raise HeaderError("kind-conflict", f"conflicting existing header state: {parsed.state}")
        header_lines = build_original_header_lines(year)
    else:
        if not spdx or not valid_expression(spdx) or spdx in (SPDX_APACHE, SPDX_DUAL):
            raise HeaderError("invalid-external-spdx", f"invalid external SPDX: {spdx!r}")
        if parsed.state == "EXTERNAL" and parsed.spdx == spdx:
            return text
        if parsed.state != "NONE":
            raise HeaderError("kind-conflict", f"conflicting existing header state: {parsed.state}")
        header_lines = build_pekko_header_lines(spdx, notice_lines)

    nl = newline_of(text)
    header_text = nl.join(header_lines)
    if parsed.body == "":
        return parsed.bom + parsed.shebang + header_text + nl
    return parsed.bom + parsed.shebang + header_text + nl + nl + parsed.body


def remove_header(text, *, allowed_external=()):
    """ツール管理ヘッダーを除去する。外部ヘッダーは飛ばし、未知形式は拒否する（I1, I5, F4, M3）。"""
    parsed = parse_header(text, allowed_external=allowed_external)
    if parsed.state == "NONE":
        return text, "none"
    if parsed.state == "EXTERNAL":
        return text, "skipped-external"
    if parsed.state == "UNRECOGNIZED":
        raise HeaderError("unrecognized-header", "refusing to remove unrecognized SPDX header block")
    return parsed.bom + parsed.shebang + parsed.body, "removed"


def strip_header_for_scan(text):
    """線引きスクリプト向けに、認識済みライセンスヘッダーブロックを除いた本文を返す。"""
    try:
        parsed = parse_header(text, allowed_external=EXTERNAL_EXPRESSIONS)
    except HeaderError:
        return text
    if parsed.state in ("APACHE", "DUAL", "EXTERNAL"):
        return parsed.bom + parsed.shebang + parsed.body
    return text


def extract_spdx_for_other(text):
    """`crates/` 以外のファイルから SPDX-License-Identifier の式を 1 つ拾う（無ければ None）。"""
    for content, _eol in split_lines_strict(text):
        found = OTHER_SPDX_LINE.match(content)
        if found:
            return found.group(1).strip()
    return None
