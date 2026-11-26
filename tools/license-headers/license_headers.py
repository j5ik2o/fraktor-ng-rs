#!/usr/bin/env python3
"""#206: crates/**/*.rs へのライセンスヘッダー適用・除去・検査（方針 C + (a)）。

使い方:
    python3 tools/license-headers/license_headers.py
    python3 tools/license-headers/license_headers.py --apply --manifest docs/design/issue-206-file-origin.tsv
    python3 tools/license-headers/license_headers.py --apply --kind pekko [--source <pekko-path>] <path...>
    python3 tools/license-headers/license_headers.py --apply --kind original <path...>
    python3 tools/license-headers/license_headers.py --remove
    python3 tools/license-headers/license_headers.py --check [--manifest docs/design/issue-206-file-origin.tsv]
"""
import argparse
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from classify_file_origin import BINARY_SUFFIXES, MAX_BYTES, SELF_EXCLUDE_DIRS, SELF_EXCLUDE_FILES, SKIP_DIRS
from header_check import check_file, run_check_mode, run_remove_mode
from header_core import (
    EXTERNAL_EXPRESSIONS,
    HELD,
    SPDX_APACHE,
    HeaderError,
    apply_header,
    mentions_mit,
    notice_hash,
    parse_header,
    valid_expression,
)
from notice_render import PekkoGitReader, render_pekko_notice_lines
from origin_decisions import DECISIONS_DEFAULT, MANIFEST_COLUMNS, apply_decisions, load_decisions, read_manifest


def tracked_files(root):
    """`git ls-files -z` で追跡ファイルを列挙し `(crates_rs, other_files)` を返す（L7）。"""
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], check=True, capture_output=True, text=True)
    crates_rs = []
    others = []
    for rel in sorted(item for item in out.stdout.split("\0") if item):
        path = root / rel
        if not path.is_file():
            continue
        parts = Path(rel).parts
        if parts[0] == "crates":
            if path.suffix == ".rs":
                crates_rs.append(rel)
            continue
        if any(part in SKIP_DIRS for part in parts):
            continue
        if rel in SELF_EXCLUDE_FILES or rel.startswith(SELF_EXCLUDE_DIRS):
            continue
        if path.suffix in BINARY_SUFFIXES or path.stat().st_size > MAX_BYTES:
            continue
        others.append(rel)
    return crates_rs, others


def read_utf8(path):
    try:
        with path.open("r", encoding="utf-8", newline="") as handle:
            return handle.read()
    except UnicodeDecodeError as err:
        raise HeaderError("invalid-utf8", f"not valid UTF-8: {err}") from err


def write_utf8(path, text):
    with path.open("w", encoding="utf-8", newline="") as handle:
        handle.write(text)


def load_decisions_safe(path):
    try:
        return load_decisions(path), None
    except SystemExit as err:
        return None, str(err)


def allowed_external_map(decisions):
    mapping = {}
    for _num, target, spdx, *_rest in decisions:
        if not target.startswith("@") and spdx in EXTERNAL_EXPRESSIONS:
            mapping.setdefault(target, []).append(spdx)
    return mapping


def mit_decided_set(decisions):
    return {
        target for _num, target, spdx, *_rest in decisions if not target.startswith("@") and mentions_mit(spdx)
    }


def classify_state(text, allowed_external):
    try:
        parsed = parse_header(text, allowed_external=allowed_external)
    except HeaderError:
        return "invalid"
    if parsed.state == "NONE":
        return "missing"
    if parsed.state == "UNRECOGNIZED":
        return "invalid"
    errs = check_file(text, allowed_external=allowed_external)
    if any(code == "origin-conflict" for code, _ in errs):
        return "conflict"
    if errs:
        return "invalid"
    return {"APACHE": "apache", "DUAL": "dual", "EXTERNAL": "external"}[parsed.state]


def print_list(items):
    for rel, state in items:
        print(f"{state} {rel}")


def run_dry_run(root, decisions_path, list_files):
    decisions, err = load_decisions_safe(decisions_path)
    if err:
        print(f"ERROR decisions {err}", file=sys.stderr)
        return 1
    ext_map = allowed_external_map(decisions)
    crates_rs, _others = tracked_files(root)
    counts = {"apache": 0, "dual": 0, "external": 0, "missing": 0, "invalid": 0, "conflict": 0}
    listing = []
    for rel in crates_rs:
        try:
            state = classify_state(read_utf8(root / rel), ext_map.get(rel, ()))
        except HeaderError:
            state = "invalid"
        counts[state] += 1
        listing.append((rel, state))
    print(
        f"crates: {len(crates_rs)}  apache: {counts['apache']}  dual: {counts['dual']}  "
        f"external: {counts['external']}  missing: {counts['missing']}  "
        f"invalid: {counts['invalid']}  conflict: {counts['conflict']}"
    )
    if list_files:
        print_list(listing)
    return 0


def _split_semicolon(value):
    return [item for item in value.split(";") if item]


def _plan_manifest_row(ctx, rel, cells):
    spdx = cells.get("付けるSPDX", "")
    old_text = read_utf8(ctx["root"] / rel)
    if spdx == HELD:
        return (rel, old_text, old_text, "held")
    if not valid_expression(spdx):
        raise HeaderError("spdx-grammar", f"actual={spdx!r}")
    notice_lines = None
    if spdx == SPDX_APACHE or spdx in EXTERNAL_EXPRESSIONS:
        sources = _split_semicolon(cells.get("原典ファイル", ""))
        third = _split_semicolon(cells.get("第三者の表示", ""))
        if (sources or any(lbl.endswith((".scala）", ".java）")) for lbl in third)) and ctx["reader"] is None:
            ctx["reader"] = PekkoGitReader(ctx["pekko_repo"])
        reader = ctx["reader"]
        notice_lines = render_pekko_notice_lines(
            sources,
            third,
            reader.read_source if reader else None,
            reader.third_party_blocks if reader else None,
        )
        expected_hash = cells.get("表示のハッシュ", "")
        if expected_hash and notice_hash(notice_lines) != expected_hash:
            raise HeaderError("notice-hash-mismatch", f"expected={expected_hash} actual={notice_hash(notice_lines)}")
        kind = "pekko" if spdx == SPDX_APACHE else "external"
    else:
        kind = "original"

    allowed = ctx["ext_map"].get(rel, ())
    new_text = apply_header(
        old_text, kind, spdx=spdx, notice_lines=notice_lines, year=ctx["year"], allowed_external=allowed
    )
    return (rel, old_text, new_text, classify_state(new_text, allowed))


def run_apply_manifest(root, manifest_path, decisions_path, pekko_repo, year, list_files):
    decisions, err = load_decisions_safe(decisions_path)
    if err:
        print(f"ERROR decisions {err}", file=sys.stderr)
        return 1
    manifest = read_manifest(manifest_path)
    rows = [tuple(cells.get(col, "") for col in MANIFEST_COLUMNS) for _n, cells in manifest]
    try:
        decided = apply_decisions(rows, decisions, decisions_path.name)
    except SystemExit as exc:
        print(f"ERROR decisions {exc}", file=sys.stderr)
        return 1

    errors = []
    seen = set()
    crates_rows = {}
    for number, cells in manifest:
        rel = cells.get("path", "")
        if rel in seen:
            errors.append(f"ERROR manifest-duplicate-path {rel} line={number}")
        seen.add(rel)
        actual = (cells.get("付けるSPDX", ""), cells.get("決定の出典", ""))
        if actual != decided[rel]:
            errors.append(f"ERROR manifest-tampered {rel} line={number} expected={decided[rel]} actual={actual}")
        if cells.get("走査範囲") == "crates":
            crates_rows[rel] = cells

    crates_rs, _others = tracked_files(root)
    for extra in sorted(set(crates_rows) - set(crates_rs)):
        errors.append(f"ERROR manifest-missing-file {extra}")
    for missing in sorted(set(crates_rs) - set(crates_rows)):
        errors.append(f"ERROR manifest-unlisted-file {missing}")

    ctx = {"root": root, "ext_map": allowed_external_map(decisions), "reader": None, "pekko_repo": pekko_repo, "year": year}
    planned = []
    for rel in crates_rs:
        if rel not in crates_rows:
            continue
        try:
            planned.append(_plan_manifest_row(ctx, rel, crates_rows[rel]))
        except (HeaderError, FileNotFoundError, subprocess.CalledProcessError) as exc:
            code = exc.code if isinstance(exc, HeaderError) else "upstream-source"
            errors.append(f"ERROR {code} {rel} {exc}")

    if errors:
        for line in errors:
            print(line, file=sys.stderr)
        return 1

    added = already = held = 0
    for rel, old_text, new_text, state in planned:
        if state == "held":
            held += 1
        elif new_text != old_text:
            write_utf8(root / rel, new_text)
            added += 1
        else:
            already += 1
    print(f"crates: {len(crates_rs)}  added: {added}  already headed: {already}  held: {held}")
    if list_files:
        print_list([(rel, state) for rel, _old, _new, state in planned])
    return 0


def _rel_posix(root, raw_path):
    candidate = Path(raw_path)
    resolved = candidate.resolve() if candidate.is_absolute() else (root / candidate).resolve()
    try:
        return resolved.relative_to(root).as_posix(), resolved
    except ValueError as err:
        raise HeaderError("outside-repo", f"path outside root: {raw_path}") from err


def run_apply_kind(root, decisions_path, pekko_repo, args):
    if not args.paths:
        print("ERROR missing-paths specify at least one file with --apply --kind", file=sys.stderr)
        return 1
    decisions, err = load_decisions_safe(decisions_path)
    if err:
        print(f"ERROR decisions {err}", file=sys.stderr)
        return 1
    ext_map = allowed_external_map(decisions)
    mit_allowed = mit_decided_set(decisions)

    notice_lines = None
    if args.kind == "pekko" and args.source:
        try:
            reader = PekkoGitReader(pekko_repo)
            notice_lines = render_pekko_notice_lines(args.source, (), reader.read_source, reader.third_party_blocks)
        except (FileNotFoundError, subprocess.CalledProcessError) as exc:
            print(f"ERROR upstream-source {exc}", file=sys.stderr)
            return 1

    errors = []
    planned = []
    for raw in args.paths:
        try:
            rel, abs_path = _rel_posix(root, raw)
            if args.kind == "original" and rel not in mit_allowed:
                raise HeaderError("mit-without-decision", "no MIT OR Apache-2.0 decision row for this path")
            old_text = read_utf8(abs_path)
            new_text = apply_header(
                old_text, args.kind, notice_lines=notice_lines, year=args.year, allowed_external=ext_map.get(rel, ())
            )
            planned.append((rel, abs_path, old_text, new_text, classify_state(new_text, ext_map.get(rel, ()))))
        except (HeaderError, FileNotFoundError) as exc:
            code = exc.code if isinstance(exc, HeaderError) else "missing-file"
            errors.append(f"ERROR {code} {raw} {exc}")

    if errors:
        for line in errors:
            print(line, file=sys.stderr)
        return 1

    added = already = 0
    for _rel, abs_path, old_text, new_text, _state in planned:
        if new_text != old_text:
            write_utf8(abs_path, new_text)
            added += 1
        else:
            already += 1
    print(f"files: {len(planned)}  added: {added}  already headed: {already}")
    if args.list:
        print_list([(rel, state) for rel, _abs, _old, _new, state in planned])
    return 0


def _helpers():
    return {
        "load_decisions_safe": load_decisions_safe,
        "allowed_external_map": allowed_external_map,
        "tracked_files": tracked_files,
        "read_utf8": read_utf8,
        "write_utf8": write_utf8,
        "classify_state": classify_state,
    }


def parse_args(argv):
    repo_default = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=repo_default, help="リポジトリのルート")
    parser.add_argument("--manifest", type=Path, default=None, help="線引き一覧の TSV")
    parser.add_argument("--decisions", type=Path, default=DECISIONS_DEFAULT, help="人の決定のファイル")
    parser.add_argument(
        "--pekko",
        type=Path,
        default=Path(os.environ.get("PEKKO_REPO") or repo_default / "pekko"),
        help="原典 Pekko の git リポジトリ",
    )
    parser.add_argument("--kind", choices=("pekko", "original"), default=None, help="個別適用の種別")
    parser.add_argument("--source", action="append", default=[], help="--kind pekko の原典ファイル（複数指定可）")
    parser.add_argument("--year", type=int, default=None, help="--kind original の年（既定は今年）")
    parser.add_argument("--list", action="store_true", help="各ファイルの状態を出す")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--apply", action="store_true", help="ヘッダーを付ける")
    mode.add_argument("--remove", action="store_true", help="ツール管理ヘッダーを消す")
    mode.add_argument("--check", action="store_true", help="ヘッダーを検査する")
    parser.add_argument("paths", nargs="*", help="--apply --kind の対象ファイル")
    return parser.parse_args(argv)


def _resolve_under(root, path):
    return None if path is None else (path if path.is_absolute() else root / path)


def main(argv=None):
    args = parse_args(argv)
    root = args.root.resolve()
    decisions_path = _resolve_under(root, args.decisions)
    manifest_path = _resolve_under(root, args.manifest)

    if args.apply:
        if manifest_path is not None:
            if args.kind or args.paths:
                print("ERROR cli --manifest cannot be combined with --kind or positional paths", file=sys.stderr)
                return 1
            return run_apply_manifest(root, manifest_path, decisions_path, args.pekko.resolve(), args.year, args.list)
        if args.kind is None:
            print("ERROR cli --apply requires either --manifest or --kind", file=sys.stderr)
            return 1
        return run_apply_kind(root, decisions_path, args.pekko.resolve(), args)
    if args.remove:
        rc, out_lines, err_lines = run_remove_mode(root, decisions_path, args.list, _helpers())
        for line in out_lines:
            print(line)
        for line in err_lines:
            print(line, file=sys.stderr)
        return rc
    if args.check:
        rc, out_lines, err_lines = run_check_mode(root, manifest_path, decisions_path, args.list, _helpers())
        for line in out_lines:
            print(line)
        for line in err_lines:
            print(line, file=sys.stderr)
        return rc
    return run_dry_run(root, decisions_path, args.list)


if __name__ == "__main__":
    sys.exit(main())
