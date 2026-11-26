"""#206 ヘッダー検査規則 C1〜C6 の実装（§3.4.4）。"""
from header_core import (
    ANY_SPDX_TOKEN,
    HASH_PROSPECTOR_LABEL,
    HELD,
    PEKKO_TRAILER_LINES,
    RUST_COMMENT_LINE,
    SCALA_LIBRARY_LABEL,
    HeaderError,
    extract_spdx_for_other,
    has_c3_pekko_marker,
    mentions_mit,
    notice_hash,
    parse_header,
    split_lines_strict,
    valid_expression,
)
from origin_decisions import MANIFEST_COLUMNS, apply_decisions, manifest_errors, read_manifest

HINT_APPLY = "Hint: run `python3 tools/license-headers/license_headers.py --apply --kind pekko <path>`"


def _check_c6(parsed, expected_notice_hash, third_party_labels):
    errors = []
    for label in third_party_labels:
        if label.endswith((".scala）", ".java）")):
            needle = label.rsplit("（", 1)[0]
            if not any(needle in line for line in parsed.header_lines):
                errors.append(("notice-missing", label))
        elif label == SCALA_LIBRARY_LABEL:
            if not any("Copyright EPFL and Lightbend, Inc." in line for line in parsed.header_lines):
                errors.append(("notice-missing", label))
        elif label == HASH_PROSPECTOR_LABEL:
            has_comment = any(
                RUST_COMMENT_LINE.match(content) and "hash-prospector" in content.lower()
                for content, _eol in split_lines_strict(parsed.body)
            )
            if not has_comment:
                errors.append(("notice-missing", label))
    if expected_notice_hash:
        trailer_len = len(PEKKO_TRAILER_LINES)
        valid_shape = (
            len(parsed.header_lines) >= 2 + trailer_len
            and parsed.header_lines[1] == "//"
            and parsed.header_lines[-trailer_len:] == PEKKO_TRAILER_LINES
        )
        actual_hash = notice_hash(parsed.header_lines[2:-trailer_len]) if valid_shape else "invalid-structure"
        if actual_hash != expected_notice_hash:
            errors.append(("notice-mismatch", f"expected={expected_notice_hash} actual={actual_hash}"))
    return errors


def check_file(text, *, allowed_external=(), allow_missing=False, expected_notice_hash="", third_party_labels=()):
    """単一 Rust ファイルの検査規則 C1〜C4・C6 を評価し `(code, detail)` のリストを返す。"""
    try:
        parsed = parse_header(text, allowed_external=allowed_external)
    except HeaderError as err:
        return [(err.code, err.message)]

    if parsed.state == "NONE":
        if ANY_SPDX_TOKEN.search(parsed.body):
            return [("duplicate-spdx", "SPDX-License-Identifier found in body without header block")]
        return [] if allow_missing else [("missing-spdx", "missing SPDX-License-Identifier header")]

    errors = []
    if parsed.state == "UNRECOGNIZED":
        errors.append(("unrecognized-header", f"unrecognized SPDX header block ({parsed.spdx!r})"))
        if not valid_expression(parsed.spdx):
            errors.append(("spdx-grammar", f"actual={parsed.spdx!r}"))
        if mentions_mit(parsed.spdx) and has_c3_pekko_marker(parsed.body):
            errors.append(("origin-conflict", "MIT expression on file with Pekko marker"))
        return errors

    if not valid_expression(parsed.spdx):
        errors.append(("spdx-grammar", f"actual={parsed.spdx!r}"))
    if ANY_SPDX_TOKEN.search(parsed.body):
        errors.append(("duplicate-spdx", "duplicate SPDX-License-Identifier in body"))
    if parsed.body != "" and not parsed.has_separator_blank:
        errors.append(("missing-separator-blank", "missing blank line between header and body"))

    joined = "\n".join(parsed.header_lines)
    if parsed.state in ("APACHE", "EXTERNAL"):
        required = ("Licensed to the Apache Software Foundation (ASF)", "LICENSE-APACHE", "NOTICE")
        if parsed.anchor_kind != "pekko" or not all(item in joined for item in required):
            errors.append(("missing-required-notice", "missing required Apache-2.0 notice elements"))
    elif parsed.state == "DUAL":
        required = ("Copyright", "LICENSE-APACHE", "LICENSE-MIT")
        if parsed.anchor_kind != "original" or not all(item in joined for item in required):
            errors.append(("missing-required-notice", "missing required dual-license notice elements"))

    if mentions_mit(parsed.spdx) and has_c3_pekko_marker(parsed.body):
        errors.append(("origin-conflict", "MIT expression on file with Pekko marker"))

    if parsed.state in ("APACHE", "EXTERNAL") and (expected_notice_hash or third_party_labels):
        errors.extend(_check_c6(parsed, expected_notice_hash, third_party_labels))
    return errors


def _split_semicolon(value):
    return [item for item in value.split(";") if item]


def _check_crates_files(root, crates_rs, ext_map, manifest_by_path, decided, read_utf8):
    counts = {"apache": 0, "dual": 0, "external": 0, "held": 0, "missing": 0}
    errors = []
    seen_keys = set()
    spdx_by_path = {}
    listing = []

    def add_error(key, message):
        if key not in seen_keys:
            seen_keys.add(key)
            errors.append(message)

    for rel in crates_rs:
        try:
            text = read_utf8(root / rel)
        except HeaderError as herr:
            spdx_by_path[rel] = None
            add_error((herr.code, rel), f"ERROR {herr.code} {rel} {herr.message}")
            listing.append((rel, "invalid"))
            continue

        expected_spdx = decided[rel][0] if rel in decided else None
        allow_missing = expected_spdx == HELD
        cells = manifest_by_path.get(rel)
        expected_hash = cells.get("表示のハッシュ", "") if cells and expected_spdx != HELD else ""
        third_labels = _split_semicolon(cells.get("第三者の表示", "")) if cells and expected_spdx != HELD else ()

        parsed = None
        try:
            parsed = parse_header(text, allowed_external=ext_map.get(rel, ()))
            spdx_by_path[rel] = parsed.spdx
        except HeaderError:
            spdx_by_path[rel] = None

        file_errs = check_file(
            text,
            allowed_external=ext_map.get(rel, ()),
            allow_missing=allow_missing,
            expected_notice_hash=expected_hash,
            third_party_labels=third_labels,
        )
        for code, detail in file_errs:
            add_error((code, rel, detail), f"ERROR {code} {rel} {detail}")

        if parsed is None or file_errs:
            if parsed is not None and parsed.state == "NONE" and not allow_missing:
                counts["missing"] += 1
                listing.append((rel, "missing"))
            else:
                listing.append((rel, "invalid"))
        elif parsed.state == "NONE":
            counts["held"] += 1
            listing.append((rel, "held"))
        else:
            key = {"APACHE": "apache", "DUAL": "dual", "EXTERNAL": "external"}[parsed.state]
            counts[key] += 1
            listing.append((rel, key))
    return counts, errors, seen_keys, spdx_by_path, listing


def run_remove_mode(root, decisions_path, list_files, helpers):
    """`--remove` を実行し `(exit_code, stdout_lines, stderr_lines)` を返す（F4, I6, M3）。"""
    from header_core import remove_header

    decisions, err = helpers["load_decisions_safe"](decisions_path)
    if err:
        return 1, [], [f"ERROR decisions {err}"]
    ext_map = helpers["allowed_external_map"](decisions)
    crates_rs, _others = helpers["tracked_files"](root)
    errors = []
    planned = []
    for rel in crates_rs:
        try:
            old_text = helpers["read_utf8"](root / rel)
            new_text, action = remove_header(old_text, allowed_external=ext_map.get(rel, ()))
            planned.append((rel, old_text, new_text, action))
        except HeaderError as herr:
            errors.append(f"ERROR {herr.code} {rel} {herr.message}")

    if errors:
        return 1, errors, []

    removed = without = 0
    skipped = []
    listing = []
    for rel, _old, new_text, action in planned:
        if action == "removed":
            helpers["write_utf8"](root / rel, new_text)
            removed += 1
        elif action == "skipped-external":
            skipped.append(rel)
        else:
            without += 1
        listing.append((rel, helpers["classify_state"](new_text, ext_map.get(rel, ()))))

    out = [f"crates: {len(crates_rs)}  removed: {removed}  without header: {without}  skipped external: {len(skipped)}"]
    out.extend(f"SKIP external {rel}" for rel in skipped)
    if list_files:
        out.extend(f"{state} {rel}" for rel, state in listing)
    return 0, out, []


def run_check_mode(root, manifest_path, decisions_path, list_files, helpers):
    """`--check` および `--check --manifest` を実行し `(exit_code, stdout_lines, stderr_lines)` を返す。"""
    decisions, err = helpers["load_decisions_safe"](decisions_path)
    if err:
        return 1, [f"ERROR decisions {err}"], []
    ext_map = helpers["allowed_external_map"](decisions)
    crates_rs, others = helpers["tracked_files"](root)

    manifest = None
    manifest_by_path = {}
    decided = {}
    if manifest_path is not None:
        manifest = read_manifest(manifest_path)
        rows = [tuple(cells.get(col, "") for col in MANIFEST_COLUMNS) for _n, cells in manifest]
        try:
            decided = apply_decisions(rows, decisions, decisions_path.name)
        except SystemExit as exc:
            return 1, [f"ERROR decisions {exc}"], []
        manifest_by_path = {cells.get("path", ""): cells for _n, cells in manifest}

    counts, errors, seen_keys, spdx_by_path, listing = _check_crates_files(
        root, crates_rs, ext_map, manifest_by_path, decided, helpers["read_utf8"]
    )

    if manifest is not None:
        other_targets = set(others) | {
            cells.get("path", "") for _n, cells in manifest if cells.get("走査範囲") != "crates"
        }
        for rel in sorted(other_targets):
            abs_path = root / rel
            if not abs_path.is_file():
                continue
            try:
                spdx_by_path[rel] = extract_spdx_for_other(helpers["read_utf8"](abs_path))
            except HeaderError:
                spdx_by_path[rel] = None

        for merr in manifest_errors(manifest, decisions, decisions_path.name, spdx_by_path):
            parts = merr.split()
            code = parts[0]
            path = parts[1] if len(parts) > 1 else ""
            if code == "spdx-grammar" and any(k[0] == "spdx-grammar" and k[1] == path for k in seen_keys):
                continue
            key = (code, path, merr)
            if key not in seen_keys:
                seen_keys.add(key)
                errors.append(f"ERROR {merr}")

    checked_total = len(crates_rs) if manifest is None else len(spdx_by_path)
    out = [
        f"checked: {checked_total}  apache: {counts['apache']}  dual: {counts['dual']}  "
        f"external: {counts['external']}  held: {counts['held']}  "
        f"missing: {counts['missing']}  errors: {len(errors)}"
    ]
    out.extend(errors)
    if counts["missing"] > 0:
        out.append(HINT_APPLY)
    if list_files:
        out.extend(f"{state} {rel}" for rel, state in listing)
    return (1 if errors else 0), out, []
