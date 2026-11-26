#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Lists newly added `pub` items between two git refs (or base ref vs worktree).

Uses `tools/syn-check`'s `pub_api_list` binary to extract canonical public API
items for each crate under `crates/*/src` without requiring nightly rustdoc.
Intended for PR `$GITHUB_STEP_SUMMARY` reports and local checks; never fails
the build because of API additions.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass
from pathlib import Path

WORKTREE_SENTINEL = "WORKTREE"
MAX_ITEMS_PER_CRATE = 200


# The item's path in a line of `pub_api_list`, e.g. `fraktor_actor::kernel::ActorSystem::new`
# in `pub fn fraktor_actor::kernel::ActorSystem::new() -> Self`.
ITEM_PATH_RE = re.compile(r"\b[a-z_][a-z0-9_]*(?:::[A-Za-z0-9_]+)+")


@dataclass(frozen=True)
class CrateDiff:
    crate: str
    added: tuple[str, ...]
    """Items whose path the base does not have: what may collide with another PR."""
    changed: tuple[str, ...]
    """Items the base has under the same path, with another signature."""
    removed_count: int
    """Paths the base has and the head does not."""


def item_path(line: str) -> str:
    match = ITEM_PATH_RE.search(line)
    return match.group(0) if match else line


def run_git(root: Path, args: list[str]) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        capture_output=True,
    )


def detect_default_base(root: Path) -> str:
    for candidate in ("origin/main", "main", "HEAD~1"):
        res = run_git(root, ["rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}"])
        if res.returncode == 0:
            return candidate
    raise RuntimeError("could not determine a default base ref; pass --base explicitly")


def short_ref_label(root: Path, ref: str) -> str:
    if ref == WORKTREE_SENTINEL:
        return "worktree"
    res = run_git(root, ["rev-parse", "--short", ref])
    if res.returncode == 0:
        return res.stdout.decode("utf-8").strip()
    return ref


def export_crates_at_ref(root: Path, ref: str, dest: Path) -> Path:
    """Exports `crates/` at `ref` into `dest` using `git archive`, returning `dest/crates`."""
    dest.mkdir(parents=True, exist_ok=True)
    crates_dest = dest / "crates"
    check_tree = run_git(root, ["rev-parse", "--verify", "--quiet", f"{ref}:crates"])
    if check_tree.returncode != 0:
        crates_dest.mkdir(parents=True, exist_ok=True)
        return crates_dest

    archive = run_git(root, ["archive", "--format=tar", ref, "crates"])
    if archive.returncode != 0:
        stderr = archive.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git archive failed for {ref}: {stderr}")

    tar_path = dest / "crates.tar"
    tar_path.write_bytes(archive.stdout)
    with tarfile.open(tar_path, mode="r") as tf:
        if hasattr(tarfile, "data_filter"):
            tf.extractall(dest, filter="data")
        else:
            tf.extractall(dest)
    tar_path.unlink()
    crates_dest.mkdir(parents=True, exist_ok=True)
    return crates_dest


def list_crate_dirs(crates_root: Path) -> list[Path]:
    if not crates_root.is_dir():
        return []
    return sorted(p for p in crates_root.iterdir() if p.is_dir() and (p / "src" / "lib.rs").is_file())


def collect_pub_api(repo_root: Path, crates_root: Path) -> dict[str, list[str]]:
    crate_dirs = list_crate_dirs(crates_root)
    if not crate_dirs:
        return {}
    manifest = repo_root / "tools" / "syn-check" / "Cargo.toml"
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "--locked",
        "--manifest-path",
        str(manifest),
        "--bin",
        "pub_api_list",
        "--",
        *(str(d) for d in crate_dirs),
    ]
    res = subprocess.run(cmd, check=False, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"pub_api_list failed:\n{res.stderr.strip()}")
    return json.loads(res.stdout)


def compute_diffs(base_api: dict[str, list[str]], head_api: dict[str, list[str]]) -> list[CrateDiff]:
    diffs: list[CrateDiff] = []
    for crate in sorted(set(base_api) | set(head_api)):
        base_lines = set(base_api.get(crate, []))
        head_lines = set(head_api.get(crate, []))
        base_paths = {item_path(line) for line in base_lines}
        head_paths = {item_path(line) for line in head_lines}
        new_lines = sorted(head_lines - base_lines)
        diffs.append(
            CrateDiff(
                crate=crate,
                added=tuple(line for line in new_lines if item_path(line) not in base_paths),
                changed=tuple(line for line in new_lines if item_path(line) in base_paths),
                removed_count=len(base_paths - head_paths),
            )
        )
    return diffs


def _code_block(items: tuple[str, ...]) -> list[str]:
    lines = ["```rust", *(f"+ {item}" for item in items[:MAX_ITEMS_PER_CRATE])]
    if len(items) > MAX_ITEMS_PER_CRATE:
        lines.append(f"// ... {len(items) - MAX_ITEMS_PER_CRATE} more item(s) omitted")
    return [*lines, "```", ""]


def render_markdown(diffs: list[CrateDiff], base_label: str, head_label: str) -> str:
    total_added = sum(len(d.added) for d in diffs)
    total_changed = sum(len(d.changed) for d in diffs)
    total_removed = sum(d.removed_count for d in diffs)
    lines = [
        f"## Public API Additions (`{base_label}..{head_label}`)",
        "",
        f"- Newly added `pub` items: **{total_added}** (signature changed: {total_changed}, removed: {total_removed})",
        "- Informational only (does not fail CI). Check for overlapping `pub` items before parallel merges.",
        "",
    ]
    if total_added == 0:
        lines.extend(["No new `pub` items added.", ""])
    for diff in diffs:
        if diff.added:
            lines.extend([f"### `{diff.crate}` (+{len(diff.added)})", "", *_code_block(diff.added)])
    if total_changed:
        lines.extend(["<details><summary>Signature changes of existing items</summary>", ""])
        for diff in diffs:
            if diff.changed:
                lines.extend([f"#### `{diff.crate}` ({len(diff.changed)})", "", *_code_block(diff.changed)])
        lines.extend(["</details>", ""])
    return "\n".join(lines)


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    repo_default = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description="List newly added `pub` items between two git refs.")
    parser.add_argument("--root", type=Path, default=repo_default, help="Repository root directory")
    parser.add_argument("--base", default=None, help="Base git ref (default: origin/main, main, or HEAD~1)")
    parser.add_argument(
        "--head",
        default=WORKTREE_SENTINEL,
        help="Head git ref, or WORKTREE to inspect the current working tree (default: WORKTREE)",
    )
    parser.add_argument("--summary", type=Path, default=None, help="Optional Markdown file to append (e.g. $GITHUB_STEP_SUMMARY)")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    root = args.root.resolve()
    try:
        base_ref = args.base or detect_default_base(root)
        head_ref = args.head
        with tempfile.TemporaryDirectory(prefix="fraktor-pub-api-diff-") as tmp_str:
            tmp = Path(tmp_str)
            base_crates = export_crates_at_ref(root, base_ref, tmp / "base")
            if head_ref == WORKTREE_SENTINEL:
                head_crates = root / "crates"
            else:
                head_crates = export_crates_at_ref(root, head_ref, tmp / "head")

            base_api = collect_pub_api(root, base_crates)
            head_api = collect_pub_api(root, head_crates)
    except RuntimeError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1

    diffs = compute_diffs(base_api, head_api)
    report = render_markdown(diffs, short_ref_label(root, base_ref), short_ref_label(root, head_ref))
    print(report, end="")

    if args.summary is not None and str(args.summary):
        with args.summary.open("a", encoding="utf-8") as handle:
            handle.write(report)
            if not report.endswith("\n"):
                handle.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
