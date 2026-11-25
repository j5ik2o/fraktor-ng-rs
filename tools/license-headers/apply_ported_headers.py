#!/usr/bin/env python3
"""移植したファイルの冒頭に、Apache License 2.0 の短いヘッダーを付ける。

対象は、原典（Apache Pekko）からの移植だと分かる行を冒頭 20 行に持つ
`crates/**/*.rs` だけである。マーカーは次の 3 つ。

  - `Ported from Pekko`
  - `Port of Pekko`
  - `Checked against Pekko`

ヘッダーは冪等に付く。2 回 `--apply` してもファイルは変わらない。
`--remove` は付けたヘッダーだけを消し、元の内容に戻す。

使い方:
    # 既定は dry-run。対象件数と、まだヘッダーが無い件数を出す
    python3 tools/license-headers/apply_ported_headers.py

    # ヘッダーを付ける（方針決定後に実行する。いまは実行しない）
    python3 tools/license-headers/apply_ported_headers.py --apply

    # 戻す
    python3 tools/license-headers/apply_ported_headers.py --remove

    # CI 用。ヘッダーの無い対象ファイルがあれば終了コード 1
    python3 tools/license-headers/apply_ported_headers.py --check

`--list` を付けると、各ファイルの状態を 1 行ずつ出す。
"""
import argparse
import re
import sys
from pathlib import Path

MARKER = re.compile(r"(Ported from Pekko|Port of Pekko|Checked against Pekko)", re.IGNORECASE)
MARKER_WINDOW = 20
HEADER_LINES = (
    "// SPDX-License-Identifier: Apache-2.0",
    "//",
    "// Derived from Apache Pekko, which was derived from Akka.",
    "// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>",
    "// Copyright 2022-2026 The Apache Software Foundation",
    "//",
    "// Licensed under the Apache License, Version 2.0. See LICENSE and NOTICE",
    "// at the repository root.",
    "//",
    "// This file has been modified from the Apache Pekko original.",
)
HEADER = "\n".join(HEADER_LINES)
INSERTION = HEADER + "\n\n"


def is_ported(text):
    """冒頭のコメントに移植マーカーがあるか。"""
    head = "\n".join(text.splitlines()[:MARKER_WINDOW])
    return MARKER.search(head) is not None


def has_header(text):
    """このスクリプトが付けるヘッダーで始まるか。"""
    return text.startswith(HEADER)


def add_header(text):
    """ヘッダーが無ければ付ける。あればそのまま返す（冪等）。"""
    if has_header(text):
        return text
    return INSERTION + text


def remove_header(text):
    """このスクリプトが付けたヘッダーだけを消す。無ければそのまま返す。"""
    if not text.startswith(INSERTION):
        return text
    return text[len(INSERTION):]


def ported_files(root):
    """`<root>/crates` 配下で、移植マーカーを持つ Rust ファイルを返す。"""
    files = []
    for path in sorted((root / "crates").rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if is_ported(text):
            files.append((path, text))
    return files


def report(root, entries, list_files):
    if list_files:
        for path, text in entries:
            state = "headed" if has_header(text) else "pending"
            print(f"{state} {path.relative_to(root)}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="リポジトリのルート（既定はこのスクリプトから 2 つ上）",
    )
    parser.add_argument("--list", action="store_true", help="各対象ファイルの状態を出す")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--apply", action="store_true", help="ヘッダーを付ける")
    mode.add_argument("--remove", action="store_true", help="付けたヘッダーを消す")
    mode.add_argument("--check", action="store_true", help="未適用があれば終了コード 1")
    args = parser.parse_args(argv)

    root = args.root.resolve()
    entries = ported_files(root)
    pending = [(path, text) for path, text in entries if not has_header(text)]
    headed = [(path, text) for path, text in entries if has_header(text)]

    if args.apply:
        for path, text in pending:
            path.write_text(add_header(text), encoding="utf-8")
        print(f"ported: {len(entries)}  added: {len(pending)}  already headed: {len(headed)}")
        report(root, entries, args.list)
        return 0

    if args.remove:
        for path, text in headed:
            path.write_text(remove_header(text), encoding="utf-8")
        print(f"ported: {len(entries)}  removed: {len(headed)}  without header: {len(pending)}")
        report(root, entries, args.list)
        return 0

    if args.check:
        print(f"ported: {len(entries)}  headed: {len(headed)}  pending: {len(pending)}")
        report(root, pending, args.list or bool(pending))
        return 1 if pending else 0

    print(f"ported: {len(entries)}  headed: {len(headed)}  to add: {len(pending)}")
    report(root, pending, args.list or bool(pending))
    return 0


if __name__ == "__main__":
    sys.exit(main())
